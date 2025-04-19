# PowerShell script for Windows
# Set-StrictMode -Version Latest
$ErrorActionPreference = "Continue"

# Function name definition
$FUNCTION_NAME = "runbridge-hello-world"
$REGION = "ap-northeast-1" # Tokyo region

# Get AWS account ID
Write-Host "Getting AWS account ID..."
$ACCOUNT_ID = aws sts get-caller-identity --query "Account" --output text
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Failed to get AWS credentials. Check your AWS CLI configuration."
    exit 1
}
Write-Host "Using AWS Account ID: $ACCOUNT_ID"

# Check and create Lambda role
$ROLE_NAME = "lambda-runbridge-role"
$ROLE_ARN = "arn:aws:iam::${ACCOUNT_ID}:role/${ROLE_NAME}"

# Check if role exists
Write-Host "Checking IAM role..."
$roleExists = $false
$roleInfo = aws iam get-role --role-name $ROLE_NAME 2>&1
if ($LASTEXITCODE -eq 0) {
    $roleExists = $true
    Write-Host "Role $ROLE_NAME already exists"
} else {
    Write-Host "Role does not exist, will create it"
}

if (-not $roleExists) {
    # Create role if it doesn't exist
    Write-Host "Creating IAM role ${ROLE_NAME}..."
    
    # Create trust policy document
    $policyJson = @'
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Service": "lambda.amazonaws.com"
      },
      "Action": "sts:AssumeRole"
    }
  ]
}
'@
    
    # Create temp file
    $tempFile = New-TemporaryFile
    Set-Content -Path $tempFile -Value $policyJson -Encoding ascii
    
    # Create role
    Write-Host "Creating role with policy from: $tempFile"
    $createOutput = aws iam create-role --role-name $ROLE_NAME --assume-role-policy-document file://$tempFile 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error creating role: $createOutput"
        Remove-Item -Path $tempFile -ErrorAction SilentlyContinue
        exit 1
    }
    
    # Attach basic Lambda execution policy
    Write-Host "Attaching Lambda execution policy..."
    $attachOutput = aws iam attach-role-policy --role-name $ROLE_NAME --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error attaching policy: $attachOutput"
        Remove-Item -Path $tempFile -ErrorAction SilentlyContinue
        exit 1
    }
    
    # Remove temp file
    Remove-Item -Path $tempFile -ErrorAction SilentlyContinue
    
    # Wait for IAM role to propagate
    Write-Host "Waiting for IAM role to propagate... (15 seconds)"
    Start-Sleep -Seconds 15
}

# Check if Cross tool is installed
Write-Host "Checking Cross installation..."
if (!(Get-Command cross -ErrorAction SilentlyContinue)) {
    Write-Host "Installing the Cross tool for Rust..."
    cargo install cross
}

# Build using Cross (for Amazon Linux 2)
Write-Host "Building for Lambda using cross..."
cross build --release --target x86_64-unknown-linux-gnu --features lambda --no-default-features
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Build failed"
    exit 1
}
Write-Host "Build successful!"

# Save working directory
$workingDir = Get-Location
$targetDir = "$workingDir\target\x86_64-unknown-linux-gnu\release"
$targetFile = "$targetDir\runbridge-hello-world"

# Create ZIP package
Write-Host "Creating deployment package..."
try {
    Set-Location $targetDir
    
    # リネームしてbootstrapとして保存
    Write-Host "Renaming executable to bootstrap..."
    Copy-Item -Path "$targetFile" -Destination "$targetDir\bootstrap" -Force
    
    # bootstrapファイルをzipに圧縮
    Compress-Archive -Path "$targetDir\bootstrap" -DestinationPath "$FUNCTION_NAME.zip" -Force
    
    # 一時ファイルの削除
    Remove-Item -Path "$targetDir\bootstrap" -Force
    
    Set-Location $workingDir
} catch {
    Write-Host "Error: Failed to create deployment package: $_"
    exit 1
}

$ZIP_PATH = "$targetDir\$FUNCTION_NAME.zip"

# Check if Lambda function exists
Write-Host "Checking if Lambda function exists..."
$functionInfo = aws lambda get-function --function-name $FUNCTION_NAME --region $REGION 2>&1
$functionExists = $LASTEXITCODE -eq 0

if ($functionExists) {
    Write-Host "Lambda function exists. Will update it."
    # Update existing function
    Write-Host "Updating Lambda function code..."
    $updateResult = aws lambda update-function-code --function-name $FUNCTION_NAME --zip-file fileb://$ZIP_PATH --region $REGION 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error updating function code: $updateResult"
        exit 1
    }
} else {
    Write-Host "Lambda function does not exist. Will create a new one."
    # Create new function
    Write-Host "Creating new Lambda function..."
    $createResult = aws lambda create-function --function-name $FUNCTION_NAME --runtime provided.al2 --role $ROLE_ARN --handler doesnt.matter --zip-file fileb://$ZIP_PATH --region $REGION --architectures x86_64 --environment "Variables={RUST_LOG=info}" --description "RunBridge Hello World Example" 2>&1
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error creating function: $createResult"
        Write-Host "Waiting 15 more seconds for role propagation and trying again..."
        Start-Sleep -Seconds 15
        
        $createResult = aws lambda create-function --function-name $FUNCTION_NAME --runtime provided.al2 --role $ROLE_ARN --handler doesnt.matter --zip-file fileb://$ZIP_PATH --region $REGION --architectures x86_64 --environment "Variables={RUST_LOG=info}" --description "RunBridge Hello World Example" 2>&1
        
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Error creating Lambda function: $createResult"
            Write-Host "IAM role propagation may be taking longer than expected. Try again in a minute."
            exit 1
        }
    }
}

# Set up function URL
Write-Host "Setting up function URL..."
$urlInfo = aws lambda get-function-url-config --function-name $FUNCTION_NAME --region $REGION 2>&1
$urlExists = $LASTEXITCODE -eq 0

if ($urlExists) {
    Write-Host "Function URL exists. Will update it."
    $updateResult = aws lambda update-function-url-config --function-name $FUNCTION_NAME --auth-type NONE --region $REGION --cors "AllowOrigins=['*']" 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error updating function URL: $updateResult"
    }
} else {
    Write-Host "Function URL does not exist. Will create a new one."
    $createResult = aws lambda create-function-url-config --function-name $FUNCTION_NAME --auth-type NONE --region $REGION --cors "AllowOrigins=['*']" 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Error creating function URL: $createResult"
    }
}

# Get and display function URL
Write-Host "Function URL:"
$urlResult = aws lambda get-function-url-config --function-name $FUNCTION_NAME --region $REGION --query "FunctionUrl" --output text 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-Host $urlResult
    Write-Host "Deployment completed!"
    Write-Host "Try accessing: ${urlResult}hello?name=YourName&lang=ja"
} else {
    Write-Host "Could not retrieve function URL. Please check if the function was deployed successfully."
}
