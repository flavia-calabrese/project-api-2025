# Base Linux
FROM ubuntu:22.04

# Evita domande interattive
ENV DEBIAN_FRONTEND=noninteractive

# Aggiorna e installa strumenti
RUN apt update && \
    apt install -y \
        curl \
        build-essential \
        fuse3 \
        libfuse3-dev \
        git \
        vim \
        ca-certificates \
        pkg-config \
        && rm -rf /var/lib/apt/lists/*

# Installa Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Crea cartella per montaggio e progetto
RUN mkdir -p /mnt/remote-fs /workspace
WORKDIR /workspace

# Permessi FUSE
RUN chown root:root /mnt/remote-fs && chmod 777 /mnt/remote-fs

# Avvio interattivo
CMD ["/bin/bash"]
