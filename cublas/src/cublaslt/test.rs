﻿use cuda::{memcpy_d2h, AsRaw, DevMem, Device, Stream};
use rand::Rng;

use crate::{
    bindings::{cublasComputeType_t::CUBLAS_COMPUTE_32F, cudaDataType},
    cublaslt::CublasLtMatMulLayout,
    Cublas, CublasLt, CublasLtMatMulDescriptor, CublasLtMatrix, CublasLtMatrixLayout, MatrixOrder,
};

const M: usize = 5376;
const K: usize = 2048;
const N: usize = 256;
const ALPHA: f32 = 1.;
const BETA: f32 = 0.;

fn rand_blob<'ctx>(len: usize, stream: &Stream<'ctx>) -> DevMem<'ctx> {
    let mut rng = rand::thread_rng();
    let mut mem = vec![0.0f32; len];
    rng.fill(&mut mem[..]);
    stream.from_host(&mem)
}

#[test]
fn general() {
    cuda::init();
    let Some(dev) = Device::fetch() else {
        return;
    };
    dev.context().apply(|ctx| {
        let stream = ctx.stream();
        let dev_a = rand_blob(M * K, &stream);
        let dev_b = rand_blob(K * N, &stream);
        let mut dev_c = stream.malloc::<f32>(M * N);

        let cublas = Cublas::new(ctx);
        cublas.set_stream(&stream);

        cublas!(cublasGemmEx(
            cublas.as_raw(),
            cublasOperation_t::CUBLAS_OP_N,
            cublasOperation_t::CUBLAS_OP_N,
            N as _,
            M as _,
            K as _,
            ((&ALPHA) as *const f32).cast(),
            dev_b.as_ptr() as _,
            cudaDataType_t::CUDA_R_32F,
            N as _,
            dev_a.as_ptr() as _,
            cudaDataType_t::CUDA_R_32F,
            K as _,
            ((&BETA) as *const f32).cast(),
            dev_c.as_mut_ptr() as _,
            cudaDataType_t::CUDA_R_32F,
            N as _,
            cublasComputeType_t::CUBLAS_COMPUTE_32F,
            cublasGemmAlgo_t::CUBLAS_GEMM_DFALT,
        ));
        let mut answer = vec![0.0f32; M * N];
        memcpy_d2h(&mut answer, &dev_c);

        let a_desc = CublasLtMatrix::from(CublasLtMatrixLayout {
            rows: M as _,
            cols: K as _,
            major_stride: K as _,
            order: MatrixOrder::RowMajor,
            data_type: cudaDataType::CUDA_R_32F,
            batch: 1,
            stride: 0,
        });
        let b_desc = CublasLtMatrix::from(CublasLtMatrixLayout {
            rows: K as _,
            cols: N as _,
            major_stride: N as _,
            order: MatrixOrder::RowMajor,
            data_type: cudaDataType::CUDA_R_32F,
            batch: 1,
            stride: 0,
        });
        let c_desc = CublasLtMatrix::from(CublasLtMatrixLayout {
            rows: M as _,
            cols: N as _,
            major_stride: N as _,
            order: MatrixOrder::RowMajor,
            data_type: cudaDataType::CUDA_R_32F,
            batch: 1,
            stride: 0,
        });
        let mat_mul = CublasLtMatMulDescriptor::new(CUBLAS_COMPUTE_32F, cudaDataType::CUDA_R_32F);
        let layout = CublasLtMatMulLayout {
            mat_mul: &mat_mul,
            a: &a_desc,
            b: &b_desc,
            c: &c_desc,
            d: &c_desc,
        };

        let cublaslt = CublasLt::new(ctx);
        let mut algo = cublaslt.tune(layout, usize::MAX, 1);
        let (algo, workspace_size) = algo.pop().unwrap();
        let mut workspace = stream.malloc::<u8>(workspace_size);
        cublaslt.mat_mul(
            layout,
            algo,
            dev_c.as_mut_ptr(),
            ALPHA,
            dev_a.as_ptr(),
            dev_b.as_ptr(),
            BETA,
            dev_c.as_ptr(),
            &mut *workspace,
            &stream,
        );

        let mut result = vec![0.0f32; M * N];
        memcpy_d2h(&mut result, &dev_c);
        assert_eq!(result, answer);
    });
}
