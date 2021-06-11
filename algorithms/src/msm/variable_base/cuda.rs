// Copyright (C) 2019-2021 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use std::{any::TypeId, rc::Rc, time::Instant};

use cuda_oxide::*;
use snarkvm_curves::{
    bls12_377::{Fr, G1Affine, G1Projective},
    traits::{AffineCurve, ProjectiveCurve},
};
use snarkvm_fields::{PrimeField, Zero};

pub struct CudaRequest {
    bases: Vec<G1Affine>,
    scalars: Vec<Fr>,
    response: crossbeam_channel::Sender<Result<G1Projective, ErrorCode>>,
}

struct CudaContext<'a, 'b, 'c> {
    handle: &'b Rc<Handle<'a>>,
    stream: &'b mut Stream<'a>,
    num_groups: u32,
    output_buf: DeviceBox<'a>,
    pixel_func: Function<'a, 'c>,
    row_func: Function<'a, 'c>,
}

const SCALAR_BITS: usize = 253;
const BIT_WIDTH: usize = 1;
const LIMB_COUNT: usize = 6;
const WINDOW_SIZE: u32 = 128; // must match in cuda source

#[allow(dead_code)]
#[repr(C)]
struct CudaAffine {
    x: <G1Affine as AffineCurve>::BaseField,
    y: <G1Affine as AffineCurve>::BaseField,
}

fn handle_cuda_request(context: &mut CudaContext, request: &CudaRequest) -> Result<G1Projective, ErrorCode> {
    let mapped_bases: Vec<_> = request
        .bases
        .iter()
        .map(|affine| CudaAffine {
            x: affine.x,
            y: affine.y,
        })
        .collect();

    let mut window_lengths = (0..(request.scalars.len() as u32 / WINDOW_SIZE))
        .into_iter()
        .map(|_| WINDOW_SIZE)
        .collect::<Vec<u32>>();
    let overflow_size = request.scalars.len() as u32 - window_lengths.len() as u32 * WINDOW_SIZE;
    if overflow_size > 0 {
        window_lengths.push(overflow_size);
    }

    let window_lengths_buf = DeviceBox::new_ffi(&context.handle, &window_lengths[..])?;
    let bases_in_buf = DeviceBox::new_ffi(&context.handle, &mapped_bases[..])?;
    let scalars_in_buf = DeviceBox::new_ffi(&context.handle, &request.scalars[..])?;
    context.output_buf.memset_d32_stream(0, context.stream)?;

    let buckets = DeviceBox::alloc(
        &context.handle,
        context.num_groups as u64 * window_lengths.len() as u64 * 8 * LIMB_COUNT as u64 * 3,
    )?;

    let start = Instant::now();

    context.stream.launch(
        &context.pixel_func,
        window_lengths.len() as u32,
        context.num_groups,
        0,
        (
            &buckets,
            &bases_in_buf,
            &scalars_in_buf,
            &window_lengths_buf,
            window_lengths.len() as u32,
        ),
    )?;

    context.stream.sync()?;

    let time = (start.elapsed().as_micros() as f64) / 1000.0;
    println!("msm-pixel took {} ms", time);

    context.stream.launch(
        &context.row_func,
        1,
        context.num_groups,
        0,
        (&context.output_buf, &buckets, window_lengths.len() as u32),
    )?;

    context.stream.sync()?;

    let time = (start.elapsed().as_micros() as f64) / 1000.0;
    println!("msm-row took {} ms", time);

    let mut out = context.output_buf.load()?;

    let base_size = std::mem::size_of::<<<G1Affine as AffineCurve>::BaseField as PrimeField>::BigInteger>();

    let windows = unsafe {
        Vec::from_raw_parts(
            out.as_mut_ptr() as *mut G1Projective,
            out.len() / base_size / 3,
            out.capacity() / base_size / 3,
        )
    };
    std::mem::forget(out);

    let lowest = windows.first().unwrap();

    // We're traversing windows from high to low.
    let out = windows[1..]
        .iter()
        .rev()
        .fold(G1Projective::zero(), |mut total, sum_i| {
            total += sum_i;
            for _ in 0..BIT_WIDTH {
                total.double_in_place();
            }
            total
        })
        + lowest;
    Ok(out)
}

fn cuda_thread(input: crossbeam_channel::Receiver<CudaRequest>) {
    let num_groups = (SCALAR_BITS + BIT_WIDTH - 1) / BIT_WIDTH;
    Cuda::init().unwrap();

    let mut devices = Cuda::list_devices().unwrap();
    if devices.is_empty() {
        eprintln!("CUDA enabled but no CUDA devices were found");
        return;
    }
    let device = devices.remove(0);
    eprintln!("Using '{}' as CUDA device", device.name().unwrap());
    let mut ctx = Context::new(&device).unwrap();
    #[cfg(debug_assertions)]
    ctx.set_limit(LimitType::PrintfFifoSize, 1024 * 1024 * 16).unwrap();
    let handle = ctx.enter().unwrap();
    let module = Module::load(&handle, include_bytes!("./blst_377_cuda/kernel")).unwrap();
    let pixel_func = module.get_function("msm6_pixel").unwrap();
    let row_func = module.get_function("msm6_collapse_rows").unwrap();
    let mut stream = Stream::new(&handle).unwrap();

    let output_buf = DeviceBox::alloc(&handle, LIMB_COUNT as u64 * 8 * num_groups as u64 * 3).unwrap();

    let mut context = CudaContext {
        handle: &handle,
        stream: &mut stream,
        num_groups: num_groups as u32,
        output_buf,
        pixel_func,
        row_func,
    };

    while let Ok(request) = input.recv() {
        let out = handle_cuda_request(&mut context, &request);

        request.response.send(out).ok();
    }
}

lazy_static::lazy_static! {
    static ref CUDA_DISPATCH: crossbeam_channel::Sender<CudaRequest> = {
        let (sender, receiver) = crossbeam_channel::bounded(16);
        std::thread::spawn(move || cuda_thread(receiver));
        sender
    };
}

pub(super) fn msm_cuda<G: AffineCurve>(
    mut bases: &[G],
    mut scalars: &[<G::ScalarField as PrimeField>::BigInteger],
) -> Result<G::Projective, ErrorCode> {
    if TypeId::of::<G>() != TypeId::of::<G1Affine>() {
        unimplemented!("trying to use cuda for unsupported curve");
    }

    if bases.len() < scalars.len() {
        scalars = &scalars[..bases.len()];
    } else if bases.len() > scalars.len() {
        bases = &bases[..scalars.len()];
    }

    if scalars.len() < 4 {
        let mut acc = G::Projective::zero();

        for (base, scalar) in bases.iter().zip(scalars.iter()) {
            acc += &base.mul(*scalar);
        }
        return Ok(acc);
    }

    let (sender, receiver) = crossbeam_channel::bounded(1);
    CUDA_DISPATCH
        .send(CudaRequest {
            bases: unsafe { std::mem::transmute(bases.to_vec()) },
            scalars: unsafe { std::mem::transmute(scalars.to_vec()) },
            response: sender,
        })
        .map_err(|_| ErrorCode::NoDevice)?;
    match receiver.recv() {
        Ok(x) => unsafe { std::mem::transmute_copy(&x) },
        Err(_) => Err(ErrorCode::NoDevice)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_xorshift::XorShiftRng;
    use snarkvm_curves::bls12_377::Fq;
    use snarkvm_fields::{Field, PrimeField};
    use snarkvm_utilities::UniformRand;

    use super::*;

    fn run_roundtrip<T: Clone>(name: &str, inputs: &[Vec<T>]) -> Vec<T> {
        Cuda::init().unwrap();
        let device = Cuda::list_devices().unwrap().remove(0);
        let mut ctx = Context::new(&device).unwrap();
        ctx.set_limit(LimitType::PrintfFifoSize, 1024 * 1024 * 16).unwrap();
        let handle = ctx.enter().unwrap();
        let module = Module::load(&handle, include_bytes!("./blst_377_cuda/kernel.test")).unwrap();
        let func = module.get_function(name).unwrap();
        let mut stream = Stream::new(&handle).unwrap();

        let size = std::mem::size_of::<T>();
        let mut out = vec![];

        let first_len = inputs.first().unwrap().len();
        assert!(inputs.iter().all(|x| x.len() == first_len));

        for input in inputs {
            let output_buf = DeviceBox::alloc(&handle, size as u64).unwrap();
            output_buf.memset_d32(0).unwrap();

            let input_buf = DeviceBox::new_ffi(&handle, &input[..]).unwrap();

            stream.launch(&func, 1, 1, 0, (&output_buf, &input_buf)).unwrap();

            stream.sync().unwrap();

            let output = output_buf.load().unwrap();
            let output = unsafe { (output.as_ptr() as *const T).as_ref() }.unwrap();
            out.push(output.clone());
        }

        out
    }

    fn make_tests(count: usize, cardinality: usize) -> Vec<Vec<Fq>> {
        let mut rng = XorShiftRng::seed_from_u64(234832847u64);
        let mut inputs = vec![];
        for _ in 0..count {
            let mut out = vec![];
            for _ in 0..cardinality {
                out.push(Fq::rand(&mut rng));
            }
            inputs.push(out);
        }
        inputs
    }

    fn make_projective_tests(count: usize, cardinality: usize) -> Vec<Vec<G1Projective>> {
        let mut rng = XorShiftRng::seed_from_u64(234832847u64);
        let mut inputs = vec![];
        for _ in 0..count {
            let mut out = vec![];
            for _ in 0..cardinality {
                out.push(G1Projective::rand(&mut rng));
            }
            inputs.push(out);
        }
        inputs
    }

    #[test]
    fn test_cuda_mul() {
        let inputs = make_tests(1000, 2);

        let output = run_roundtrip("mul_test", &inputs[..]);
        for (input, output) in inputs.iter().zip(output.iter()) {
            let rust_out = input[0] * &input[1];
            let output = output.into_repr_raw();
            let rust_out = rust_out.into_repr_raw();

            if rust_out != output {
                eprintln!("test failed: {:?} != {:?}", rust_out.as_ref(), output.as_ref());
                eprintln!(
                    "inputs {:?}, {:?}",
                    input[0].into_repr_raw().as_ref(),
                    input[1].into_repr_raw().as_ref()
                );
                assert_eq!(rust_out.as_ref(), output.as_ref());
            }
        }
    }

    #[test]
    fn test_cuda_square() {
        let inputs = make_tests(1000, 1);

        let output = run_roundtrip("sqr_test", &inputs[..]);
        for (input, output) in inputs.iter().zip(output.iter()) {
            let rust_out = input[0].square();
            let output = output.into_repr_raw();
            let rust_out = rust_out.into_repr_raw();

            if rust_out != output {
                eprintln!("test failed: {:?} != {:?}", rust_out.as_ref(), output.as_ref());
                eprintln!("inputs {:?}", input[0].into_repr_raw().as_ref());
                assert_eq!(rust_out.as_ref(), output.as_ref());
            }
        }
    }

    #[test]
    fn test_cuda_add() {
        let inputs = make_tests(1000, 2);

        let output = run_roundtrip("add_test", &inputs[..]);

        for (input, output) in inputs.iter().zip(output.iter()) {
            let rust_out = input[0] + &input[1];
            let output = output.into_repr_raw();
            let rust_out = rust_out.into_repr_raw();

            if rust_out != output {
                eprintln!("test failed: {:?} != {:?}", rust_out.as_ref(), output.as_ref());
                eprintln!(
                    "inputs {:?}, {:?}",
                    input[0].into_repr_raw().as_ref(),
                    input[1].into_repr_raw().as_ref()
                );
                assert_eq!(rust_out.as_ref(), output.as_ref());
            }
        }
    }

    #[test]
    fn test_cuda_projective_add() {
        let inputs = make_projective_tests(1000, 2);

        let output = run_roundtrip("add_projective_test", &inputs[..]);

        for (input, output) in inputs.iter().zip(output.iter()) {
            let rust_out = input[0] + &input[1];

            assert_eq!(&rust_out, output);
        }
    }

    // #[test]
    // fn test_cuda_sub() {
    //     let mut rng = XorShiftRng::seed_from_u64(234872847u64);

    //     let input = G1Projective::rand(&mut rng);
    //     let two = Fq::from_repr_raw(BigInteger384::new([0xffffffffffff0000, 0, 0, 0, 0, 0]));

    //     let output = run_roundtrip("sub_test", &[input.x, two]);

    //     let rust_out = input.x - &two;

    //     let output = output.into_repr_raw();
    //     let rust_out = rust_out.into_repr_raw();

    //     assert_eq!(rust_out.as_ref(), output.as_ref());
    // }

    // #[test]
    // fn test_cuda_sub_wrap() {

    //     let input = Fq::from_repr_raw(BigInteger384::new([8153812714561349231, 8257634240502272872, 3309964121663164928, 14127110235610584458, 15209779385852976188, 40768380988256860]));
    //     let two = Fq::from_repr_raw(BigInteger384::new([2969935861058708186, 3214136913282752413, 16546303229048786771, 5876847618051361000, 10837191459028516831, 92459721659549085]));

    //     let output = run_roundtrip("sub_test", &[input, two]);

    //     let rust_out = input - &two;

    //     let output = output.into_repr_raw();
    //     let rust_out = rust_out.into_repr_raw();

    //     assert_eq!(rust_out.as_ref(), output.as_ref());
    // }

    // #[test]
    // fn test_cuda_div2() {

    //     let input = Fq::from_repr_raw(BigInteger384::new([8153812714561349231, 8257634240502272872, 3309964121663164928, 14127110235610584458, 15209779385852976188, 40768380988256860]));

    //     let output = run_roundtrip("div2_test", &[input]);

    //     let mut rust_out = input;
    //     rust_out.0.div2();

    //     let output = output.into_repr_raw();
    //     let rust_out = rust_out.into_repr_raw();

    //     assert_eq!(rust_out.as_ref(), output.as_ref());
    // }

    // #[test]
    // fn test_cuda_inverse() {

    //     let input = Fq::from_repr_raw(BigInteger384::new([8153812714561349231, 8257634240502272872, 3309964121663164928, 14127110235610584458, 15209779385852976188, 40768380988256860]));

    //     let output = run_roundtrip("inverse_test", &[input]);

    //     let mut rust_out = input;
    //     rust_out.inverse_in_place();

    //     let output = output.into_repr_raw();
    //     let rust_out = rust_out.into_repr_raw();

    //     assert_eq!(rust_out.as_ref(), output.as_ref());
    // }
}
