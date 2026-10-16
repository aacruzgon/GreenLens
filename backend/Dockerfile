# The backend's images: api, worker and worker-cuda (docs/infra/README.md section 3).
# Build them together with `docker buildx bake -f backend/docker-bake.hcl`; the context is backend/.
# Every base is pinned by digest. No image carries model files: /models is a read-only volume.

ARG RUST_TRIXIE=rust:1.99.1-slim-trixie@sha256:4df2ae8cfdfebdfc13a65da7f253b73d68983dc0339f2719bc17a279c913b13d
ARG RUST_BOOKWORM=rust:1.99.1-slim-bookworm@sha256:a24e292b4af7bdee351a31d3a6379972a5652f0ea22a1b6d6883324ac4f5097c
ARG DEBIAN=debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f
ARG CUDA=nvidia/cuda:13.0.3-cudnn-runtime-ubuntu24.04@sha256:14f6d08d1cd4a96effbfe3101d0b56326f552c199d05e4979ee0bd616df5811b
ARG CARGO_CHEF=0.1.78

# --- api: built on trixie for libheif 1.19 ---
FROM ${RUST_TRIXIE} AS chef-api
ARG CARGO_CHEF
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config cmake clang libheif-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --version ${CARGO_CHEF} --locked
WORKDIR /src

FROM chef-api AS plan-api
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef-api AS build-api
COPY --from=plan-api /src/recipe.json .
RUN cargo chef cook --release --locked -p api --features heic --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked -p api --features heic

# --- worker: built on bookworm (glibc 2.36) so it runs on trixie and Ubuntu 24.04 ---
FROM ${RUST_BOOKWORM} AS chef-worker
ARG CARGO_CHEF
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config cmake clang && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --version ${CARGO_CHEF} --locked
WORKDIR /src

FROM chef-worker AS plan-worker
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef-worker AS build-worker
COPY --from=plan-worker /src/recipe.json .
RUN cargo chef cook --release --locked -p worker --features cuda,migraphx,webgpu --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked -p worker --features cuda,migraphx,webgpu

# --- runtimes: UID 10100, no home, no shell login; compose adds read_only, cap_drop and no-new-privileges ---
FROM ${DEBIAN} AS api
ARG REVISION=unknown
ARG CREATED=unknown
ARG ORT_SHA256=cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57
LABEL org.opencontainers.image.source=https://github.com/aacruzgon/GreenLens \
      org.opencontainers.image.revision=${REVISION} \
      org.opencontainers.image.created=${CREATED} \
      org.opencontainers.image.title=api
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl libheif1 \
 && curl -fsSL -o /tmp/ort.tgz https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-linux-x64-1.31.0.tgz \
 && echo "${ORT_SHA256}  /tmp/ort.tgz" | sha256sum -c - \
 && mkdir -p /opt/onnxruntime && tar -xzf /tmp/ort.tgz -C /opt/onnxruntime --strip-components=1 \
 && rm /tmp/ort.tgz && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10100 --no-create-home --shell /usr/sbin/nologin pdm
COPY --from=build-api /src/target/release/api /usr/local/bin/api
ENV ORT_DYLIB_PATH=/opt/onnxruntime/lib/libonnxruntime.so.1.31.0 ORT_EP=cpu
USER 10100
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/api"]

# The VLM workers and the fallback worker: they call HTTP servers and load no ONNX model.
FROM ${DEBIAN} AS worker
ARG REVISION=unknown
ARG CREATED=unknown
LABEL org.opencontainers.image.source=https://github.com/aacruzgon/GreenLens \
      org.opencontainers.image.revision=${REVISION} \
      org.opencontainers.image.created=${CREATED} \
      org.opencontainers.image.title=worker
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10100 --no-create-home --shell /usr/sbin/nologin pdm
COPY --from=build-worker /src/target/release/worker /usr/local/bin/worker
USER 10100
ENTRYPOINT ["/usr/local/bin/worker"]

# The 3070's perception worker: the same binary beside ONNX Runtime 1.31.0 for CUDA 13.
FROM ${CUDA} AS worker-cuda
ARG REVISION=unknown
ARG CREATED=unknown
ARG ORT_CUDA_SHA256=2867dec99c6a4926e8142361b9d4d1518c16c85b854b4d41d28d65d3b2c81135
LABEL org.opencontainers.image.source=https://github.com/aacruzgon/GreenLens \
      org.opencontainers.image.revision=${REVISION} \
      org.opencontainers.image.created=${CREATED} \
      org.opencontainers.image.title=worker-cuda
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && curl -fsSL -o /tmp/ort.tgz https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-linux-x64-gpu_cuda13-1.31.0.tgz \
 && echo "${ORT_CUDA_SHA256}  /tmp/ort.tgz" | sha256sum -c - \
 && mkdir -p /opt/onnxruntime && tar -xzf /tmp/ort.tgz -C /opt/onnxruntime --strip-components=1 \
 && rm /tmp/ort.tgz && apt-get purge -y curl && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10100 --no-create-home --shell /usr/sbin/nologin pdm
COPY --from=build-worker /src/target/release/worker /usr/local/bin/worker
ENV ORT_DYLIB_PATH=/opt/onnxruntime/lib/libonnxruntime.so.1.31.0 ORT_EP=cuda
USER 10100
ENTRYPOINT ["/usr/local/bin/worker"]
