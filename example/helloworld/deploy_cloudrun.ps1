# PowerShell script for Google Cloud Run deployment
# Set-StrictMode -Version Latest
$ErrorActionPreference = "Continue"

# Configuration
$SERVICE_NAME = "runbridge-hello-world"
$REGION = "asia-northeast1" # Tokyo region

# Get default project ID
Write-Host "Getting default project ID..."
$PROJECT_ID = gcloud config get-value project 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Failed to get default project ID. Please run 'gcloud config set project YOUR_PROJECT_ID' first."
    exit 1
}

# Check if gcloud is installed
Write-Host "Checking gcloud command..."
if (!(Get-Command gcloud -ErrorAction SilentlyContinue)) {
    Write-Host "Error: gcloud CLI is not installed. Please install it from https://cloud.google.com/sdk/docs/install"
    exit 1
}

# Check login and permissions
Write-Host "Checking Google Cloud authentication status..."
$auth_status = gcloud auth list --filter=status:ACTIVE --format="value(account)" 2>&1
if ([string]::IsNullOrEmpty($auth_status)) {
    Write-Host "Not logged in to Google Cloud. Initiating login..."
    gcloud auth login
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error: Failed to login to Google Cloud."
        exit 1
    }
}

# Enable necessary APIs
Write-Host "Enabling necessary APIs..."
gcloud services enable cloudbuild.googleapis.com run.googleapis.com artifactregistry.googleapis.com
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Failed to enable APIs."
    exit 1
}

# Create Dockerfile
Write-Host "Creating Dockerfile..."
$dockerfile = @"
FROM rust:1.82 as builder
WORKDIR /usr/src/app
COPY . .
WORKDIR /usr/src/app/example/helloworld
RUN cargo build --release --no-default-features --features cloud_run

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/example/helloworld/target/release/runbridge-hello-world /usr/local/bin/
ENV RUST_LOG=info
EXPOSE 8080
CMD ["runbridge-hello-world"]
"@

# Get the script directory and move to workspace root
$scriptDir = $PSScriptRoot
$workspaceRoot = (Get-Item $scriptDir).Parent.Parent.FullName
Set-Location $workspaceRoot

# Create a temporary directory for the build context
$tempDir = Join-Path $env:TEMP "runbridge-build-$(Get-Random)"
New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

Write-Host "Creating build context at $tempDir"
Set-Content -Path "$tempDir/Dockerfile" -Value $dockerfile -Encoding UTF8

# Copy the entire project to the temporary directory
Copy-Item -Path "$workspaceRoot/*" -Destination $tempDir -Recurse -Force

# Build and push Docker image
$IMAGE_NAME = "gcr.io/${PROJECT_ID}/${SERVICE_NAME}"
Write-Host "Building Docker image: $IMAGE_NAME"
Set-Location $tempDir
gcloud builds submit --tag $IMAGE_NAME
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Failed to build Docker image."
    # Clean up
    Set-Location $scriptDir
    Remove-Item -Path $tempDir -Recurse -Force
    exit 1
}

# Deploy to Cloud Run
Write-Host "Deploying to Cloud Run..."
gcloud run deploy $SERVICE_NAME `
    --image $IMAGE_NAME `
    --platform managed `
    --region $REGION `
    --allow-unauthenticated `
    --execution-environment=gen1 `
    --cpu=0.083 `
    --memory=128Mi `
    --concurrency=1
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Failed to deploy to Cloud Run."
    # Clean up
    Set-Location $scriptDir
    Remove-Item -Path $tempDir -Recurse -Force
    exit 1
}

# Clean up
Set-Location $scriptDir
Remove-Item -Path $tempDir -Recurse -Force

# Get the URL of the deployed service
$serviceUrl = gcloud run services describe $SERVICE_NAME --region $REGION --format="value(status.url)" 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Host "Deployment completed!"
    Write-Host "Service URL: $serviceUrl"
    Write-Host "Try accessing: ${serviceUrl}/hello?name=YourName&lang=ja"
} else {
    Write-Host "Could not retrieve service URL. Please check if the service was deployed successfully in the Cloud Run console."
}