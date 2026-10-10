# PostgreSQL 18 + pgvector + pg_tviews, for the suites that exercise pg_tviews (#1391,
# #1392). Built from source: pg_tviews ships no binaries.
#
# The pin lives here and nowhere else: the tag a reader recognises, and the commit it must
# resolve to (a tag can move; a commit cannot). `tools/check-pg-tviews-pin.sh` holds the docs
# to it.
#
# Built on the image it runs on, so the extension links against the same glibc and the
# same PostgreSQL 18 minor.
ARG PG_IMAGE=pgvector/pgvector:pg18

FROM ${PG_IMAGE} AS build
ARG PG_TVIEWS_TAG=v0.1.0-beta.26
ARG PG_TVIEWS_COMMIT=ed9f9883ec85ab1f026c0b067b30d150bef3b9b3
ARG PGRX_VERSION=0.17.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential pkg-config git ca-certificates curl libclang-dev clang \
       "postgresql-server-dev-${PG_MAJOR}" \
    && rm -rf /var/lib/apt/lists/*
RUN git clone --depth 1 --branch "${PG_TVIEWS_TAG}" https://github.com/fraiseql/pg_tviews.git /src \
    && test "$(git -C /src rev-parse HEAD)" = "${PG_TVIEWS_COMMIT}"
# The toolchain pg_tviews pins (rust-toolchain.toml), installed by rustup on first use.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none
ENV PATH=/root/.cargo/bin:${PATH}
WORKDIR /src
RUN cargo install --locked cargo-pgrx --version "${PGRX_VERSION}"
RUN cargo pgrx init "--pg${PG_MAJOR}" "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config"
RUN cargo pgrx package --no-default-features --features "pg${PG_MAJOR}" \
       --pg-config "/usr/lib/postgresql/${PG_MAJOR}/bin/pg_config" --out-dir /out

FROM ${PG_IMAGE}
COPY --from=build /out/usr/share/postgresql/ /usr/share/postgresql/
COPY --from=build /out/usr/lib/postgresql/ /usr/lib/postgresql/
# pg_tviews registers its GUCs and hooks at backend start; pg_stat_statements counts the
# calls a suite asserts on.
CMD ["postgres", "-c", "shared_preload_libraries=pg_tviews,pg_stat_statements"]
