Write-Host "Building CGI..."
cargo build --release --no-default-features --features cgi

Write-Host "Build completed."
