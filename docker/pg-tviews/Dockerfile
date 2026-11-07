# PostgreSQL 18 + pgvector + pg_tviews and the jsonb_delta it requires, for the suites that
# exercise pg_tviews (#1391, #1392). Built from source: neither ships binaries.
#
# The pin lives here and nowhere else: the tag a reader recognises, and the commit it must
# resolve to (a tag can move; a commit cannot). `tools/check-pg-tviews-pin.sh` holds the docs
# to it.
#
# Built on the image it runs on, so the extension links against the same glibc and the
# same PostgreSQL 18 minor.
ARG PG_IMAGE=pgvector/pgvector:pg18

FROM ${PG_IMAGE} AS build
ARG PG_TVIEWS_TAG=v0.1.0-beta.37
ARG PG_TVIEWS_COMMIT=aaba9ff48729a8935c79945795479174a5656a5f
ARG PGRX_VERSION=0.17.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential pkg-config git ca-certificates curl libclang-dev clang \
       "postgresql-server-dev-${PG_MAJOR}" \
    && rm -rf /var/lib/apt/lists/*
# The tag is fetched and its peeled commit checked, so the checkout cannot be anything else:
# a moved tag fails the build, and a new pg_tviews commit arrives only as a deliberate change
# of the pin.
RUN git init -q /src \
    && git -C /src remote add origin https://github.com/fraiseql/pg_tviews.git \
    && git -C /src fetch -q --depth 1 origin "refs/tags/${PG_TVIEWS_TAG}" \
    && test "$(git -C /src rev-parse 'FETCH_HEAD^{commit}')" = "${PG_TVIEWS_COMMIT}" \
    && git -C /src checkout -q --detach FETCH_HEAD
# The toolchain pg_tviews pins (rust-toolchain.toml), installed by rustup on first use.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none
ENV PATH=/root/.cargo/bin:${PATH}
WORKDIR /src
RUN cargo install --locked cargo-pgrx --version "${PGRX_VERSION}"
RUN cargo pgrx init "--pg${PG_MAJOR}" "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config"
RUN cargo pgrx package --no-default-features --features "pg${PG_MAJOR}" \
       --pg-config "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config" --out-dir /out
# pg_tviews requires jsonb_delta (`requires = 'jsonb_delta'` in its control file), so
# `CREATE EXTENSION pg_tviews CASCADE` installs it. Pinned the same way: the tag a reader
# recognises, and the commit its peeled object must be. It ships no toolchain file and builds
# with pg_tviews' toolchain and pgrx.
ARG JSONB_DELTA_TAG=v0.3.0
ARG JSONB_DELTA_COMMIT=f5c495030a92fcf112be098e9ef15f8f24d4a728
RUN git init -q /jsonb_delta \
    && git -C /jsonb_delta remote add origin https://github.com/evoludigit/jsonb_delta.git \
    && git -C /jsonb_delta fetch -q --depth 1 origin "refs/tags/${JSONB_DELTA_TAG}" \
    && test "$(git -C /jsonb_delta rev-parse 'FETCH_HEAD^{commit}')" = "${JSONB_DELTA_COMMIT}" \
    && git -C /jsonb_delta checkout -q --detach FETCH_HEAD \
    && cp /src/rust-toolchain.toml /jsonb_delta/
WORKDIR /jsonb_delta
RUN cargo pgrx package --no-default-features --features "pg${PG_MAJOR}" \
       --pg-config "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config" --out-dir /out

FROM ${PG_IMAGE}
COPY --from=build /out/usr/share/postgresql/ /usr/share/postgresql/
COPY --from=build /out/usr/lib/postgresql/ /usr/lib/postgresql/
# pg_tviews registers its GUCs and hooks at backend start; pg_stat_statements counts the
# calls a suite asserts on. A NOTIFY queue of 128 pages (1 MiB, the default is 8 GiB) lets a
# suite fill it past pg_tviews' 90 % limit in a second, so that a transaction commits
# unpublished (#1604); every other suite's notifications are a few hundred bytes.
CMD ["postgres", "-c", "shared_preload_libraries=pg_tviews,pg_stat_statements", \
     "-c", "max_notify_queue_pages=128"]
