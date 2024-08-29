# to run:

USE_TLS={false if running locally | true if deployed} \
S3_AK_ID={your_aws_access_key_id} \
S3_AK_SECRET={your_aws_secret_access_key} \
S3_BUCKET={your_s3_bucket_name} \
S3_REGION={your_aws_region, eg eu-central-1} \
PORT={your_port, eg 5050} \
DOMAIN={your_domain} \
docker-compose up --build