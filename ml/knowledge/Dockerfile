# The seed-upload image (docs/infra/README.md section 3): the species reference photos and the step
# that puts them in RustFS, run once per deploy before api starts (section 4).
# Build it with `docker buildx bake -f backend/docker-bake.hcl seed-upload`: the context is
# ml/knowledge/, and the named context `migrations` is backend/migrations/, for knowledge.surql.
# No secrets: PDMSEED01's keys come from .env.seed when the container runs.

ARG PYTHON=python:3.12-slim-trixie@sha256:a6e34c598f2467ed0e9a8d349809fcd8b5c603269512df273a0bb1784edc11b1

# Make the photos from the seed and check their keys against knowledge.surql (ml/knowledge/upload.py).
# A mismatch fails the build, so no image is pushed whose photos the migration can't find.
FROM ${PYTHON} AS photos
ENV PYTHONDONTWRITEBYTECODE=1 PIP_DISABLE_PIP_VERSION_CHECK=1 PIP_NO_CACHE_DIR=1 PIP_ROOT_USER_ACTION=ignore
COPY requirements-seed-upload.txt /tmp/
RUN pip install --require-hashes --only-binary=:all: -r /tmp/requirements-seed-upload.txt
WORKDIR /app
COPY . ml/knowledge/
# There is no knowledge.surql until the first seed lands; the wildcard lets the copy match nothing.
COPY --from=migrations knowledge.surq[l] backend/migrations/
RUN python -m ml.knowledge.upload build --out /photos

# The runtime: the standard library's HTTP client and the photos. No Pillow, no embedder, no token.
FROM ${PYTHON} AS seed-upload
RUN groupadd --system --gid 10100 pdm \
 && useradd --system --uid 10100 --gid 10100 --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin pdm
COPY --from=photos /photos /photos
COPY upload.py /app/ml/knowledge/upload.py
ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
ARG REVISION=unknown
ARG CREATED=unknown
LABEL org.opencontainers.image.source=https://github.com/aacruzgon/GreenLens \
      org.opencontainers.image.revision=${REVISION} \
      org.opencontainers.image.created=${CREATED} \
      org.opencontainers.image.title=seed-upload
USER 10100
ENTRYPOINT ["python", "/app/ml/knowledge/upload.py", "push", "/photos"]
