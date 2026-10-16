# The three backend images in one BuildKit run, so the worker's builder stages run once
# (docs/infra/README.md section 3). Local: `docker buildx bake -f backend/docker-bake.hcl --load`.
# CI (images.yml, S1) sets TAG, REVISION and CREATED and pushes.

variable "REGISTRY" {
  default = "ghcr.io/aacruzgon/greenlens"
}

variable "TAG" {
  default = "local"
}

variable "REVISION" {
  default = "unknown"
}

variable "CREATED" {
  default = "unknown"
}

group "default" {
  targets = ["api", "worker", "worker-cuda"]
}

target "_common" {
  context    = "backend"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  args = {
    REVISION = REVISION
    CREATED  = CREATED
  }
}

target "api" {
  inherits = ["_common"]
  target   = "api"
  tags     = ["${REGISTRY}/api:${TAG}"]
}

target "worker" {
  inherits = ["_common"]
  target   = "worker"
  tags     = ["${REGISTRY}/worker:${TAG}"]
}

target "worker-cuda" {
  inherits = ["_common"]
  target   = "worker-cuda"
  tags     = ["${REGISTRY}/worker-cuda:${TAG}"]
}
