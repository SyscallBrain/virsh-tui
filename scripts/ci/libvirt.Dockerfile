# A separate libvirt session, not the runner's daemon or the developer's VMs.
FROM rust:1.99.1-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       libvirt-daemon libvirt-daemon-driver-qemu libvirt-clients \
       qemu-system-x86 qemu-utils \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 tester \
    && mkdir /work \
    && chown tester:tester /work

ENV CARGO_HOME=/home/tester/.cargo
USER tester
WORKDIR /work
COPY --chown=tester:tester Cargo.toml Cargo.lock ./
COPY --chown=tester:tester src/ src/
COPY --chown=tester:tester benches/ benches/
COPY --chown=tester:tester tests/it_lifecycle.rs tests/it_lifecycle.rs
COPY --chown=tester:tester scripts/ci/libvirt-session.sh /usr/local/bin/test-libvirt

RUN cargo test --locked --features it --test it_lifecycle --no-run
ENTRYPOINT ["/usr/local/bin/test-libvirt"]
