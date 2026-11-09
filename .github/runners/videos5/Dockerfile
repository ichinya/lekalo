FROM node:24-bookworm-slim AS node
FROM docker:29-cli AS dockercli
FROM composer:2 AS composer
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    RUNNER_TOOL_CACHE=/opt/hostedtoolcache \
    AGENT_TOOLSDIRECTORY=/opt/hostedtoolcache \
    CARGO_BUILD_JOBS=2

RUN apt-get update \
    && apt-get install -y --no-install-recommends sudo git curl jq unzip zip xz-utils \
        build-essential pkg-config libssl-dev ca-certificates software-properties-common \
        rsync iproute2 procps libicu74 locales python3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --from=node /usr/local/lib/node_modules /usr/local/lib/node_modules
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=dockercli /usr/local/libexec/docker/cli-plugins /usr/local/libexec/docker/cli-plugins
RUN ln -s /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
    && groupadd --gid 991 ci-runner \
    && useradd --uid 994 --gid 991 --create-home --shell /bin/bash ci-runner \
    && printf 'ci-runner ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/ci-runner \
    && chmod 0440 /etc/sudoers.d/ci-runner \
    && mkdir -p /opt/hostedtoolcache \
    && chown ci-runner:ci-runner /opt/hostedtoolcache

COPY entrypoint.sh cleanup.sh trust.sh /usr/local/bin/
RUN chmod 0755 /usr/local/bin/entrypoint.sh /usr/local/bin/cleanup.sh /usr/local/bin/trust.sh
USER ci-runner
ENV HOME=/home/ci-runner \
    CARGO_HOME=/home/ci-runner/.cargo \
    RUSTUP_HOME=/home/ci-runner/.rustup \
    PATH=/home/ci-runner/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs -o /tmp/install-rust.sh \
    && sh /tmp/install-rust.sh -y --profile minimal --default-toolchain stable --component clippy,rustfmt \
    && rustup toolchain install 1.89.0 --profile minimal --component clippy \
    && rm /tmp/install-rust.sh
USER root
RUN add-apt-repository -y ppa:ondrej/php \
    && apt-get update \
    && apt-get install -y --no-install-recommends php8.3-cli php8.3-curl php8.3-mbstring \
        php8.3-xml php8.3-sqlite3 php8.3-mysql php8.3-zip php8.3-bcmath php8.3-intl php8.3-dev \
    && update-alternatives --set php /usr/bin/php8.3 \
    && mkdir -p /opt/ci-php-baseline \
    && cp -a /etc/php/8.3/cli /opt/ci-php-baseline/cli \
    && rm -rf /var/lib/apt/lists/*
COPY --from=composer /usr/bin/composer /usr/local/bin/composer
COPY --chmod=0755 prepare.sh /usr/local/bin/prepare.sh
USER ci-runner
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
