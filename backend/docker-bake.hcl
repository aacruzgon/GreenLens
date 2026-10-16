# The three backend images in one BuildKit run, so the worker's builder stages run once
# (docs/infra/README.md section 3). Local: `docker buildx bake -f backend/docker-bake.hcl --load`.
# CI's images job (.github/workflows/ci.yml) sets TAGS, REVISION and CREATED and pushes.

variable "REGISTRY" {
  default = "ghcr.io/aacruzgon/greenlens"
}

# Comma-separated, for example "sha-0123456789ab,main".
variable "TAGS" {
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
  tags     = [for t in split(",", TAGS) : "${REGISTRY}/api:${t}"]
}

target "worker" {
  inherits = ["_common"]
  target   = "worker"
  tags     = [for t in split(",", TAGS) : "${REGISTRY}/worker:${t}"]
}

target "worker-cuda" {
  inherits = ["_common"]
  target   = "worker-cuda"
  tags     = [for t in split(",", TAGS) : "${REGISTRY}/worker-cuda:${t}"]
}
