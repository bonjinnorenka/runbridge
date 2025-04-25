FROM rust:1.82 as builder
WORKDIR /usr/src/app
# ルートプロジェクトディレクトリをコピー (Dockerビルドコンテキストを親ディレクトリから開始する必要あり)
COPY . ./
# helloworld_asyncディレクトリに移動して、そこからビルド
WORKDIR /usr/src/app/example/helloworld_async
RUN cargo build --release --features cloud_run

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/example/helloworld_async/target/release/runbridge-hello-world-async /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 8080
CMD ["runbridge-hello-world-async"] 