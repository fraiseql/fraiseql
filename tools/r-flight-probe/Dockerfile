# The toolchain examples/r/fraiseql_client.R actually needs, which is more than
# `install.packages("arrow")`: R, the arrow package (for the reticulate
# conversions that turn a pyarrow Table into a data.frame), reticulate, jsonlite,
# and a Python with pyarrow for the mock server.
#
# ⚠ The first build takes about fifteen minutes: there is no prebuilt libarrow for
# this base, so `install.packages("arrow")` compiles the Arrow C++ library from
# source (LIBARROW_BINARY=true does not rescue it here — measured). Every later
# build is cached. That cost is why this probe is a manual `make probe-r-flight`
# and not a merge gate.
FROM r-base:4.4.1

RUN apt-get update && apt-get install -y --no-install-recommends \
      python3 python3-pip python3-dev && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages --no-cache-dir pyarrow

ENV LIBARROW_BINARY=true
RUN Rscript -e 'install.packages(c("reticulate", "jsonlite", "arrow"), repos = "https://cloud.r-project.org")' \
 && Rscript -e 'library(arrow); library(reticulate); library(jsonlite); cat("R deps OK\n")'

COPY mock_flight_server.py probe.R red_check.R inside.sh /probe/
CMD ["bash", "/probe/inside.sh"]
