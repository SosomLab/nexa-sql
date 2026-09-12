# nexa-sql 개발 컨테이너 — Rust stable + Oracle Instant Client(ODPI-C 런타임) + 한글 폰트 + 도구.
FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
      libaio1 unzip curl ca-certificates fonts-noto-cjk fonts-dejavu-core fonts-nanum-coding \
      pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Oracle Instant Client Basic Light(리눅스 x64 · 영구 링크 · OTN 라이선스 — 개발 컨테이너 내 사용).
RUN mkdir -p /opt/oracle && cd /opt/oracle \
    && curl -fsSL -o ic.zip https://download.oracle.com/otn_software/linux/instantclient/instantclient-basiclite-linuxx64.zip \
    && unzip -q ic.zip && rm ic.zip && mv instantclient_* instantclient \
    && echo /opt/oracle/instantclient > /etc/ld.so.conf.d/oracle-instantclient.conf && ldconfig
ENV LD_LIBRARY_PATH=/opt/oracle/instantclient

USER vscode
RUN rustup component add rustfmt clippy
