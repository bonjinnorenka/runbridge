FROM rust:1.82 as builder
WORKDIR /usr/src/app
# ルートプロジェクトディレクトリをコピー (Dockerビルドコンテキストを親ディレクトリから開始する必要あり)
COPY . ./
# helloworldディレクトリに移動して、そこからビルド
WORKDIR /usr/src/app/example/helloworld
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/example/helloworld/target/release/runbridge-hello-world /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 8080
CMD ["runbridge-hello-world"]
