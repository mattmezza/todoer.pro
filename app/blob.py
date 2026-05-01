import os
from datetime import timedelta

import boto3

STORAGE_ACCESS_KEY_ID = os.environ['TODOER_STORAGE_ACCESS_KEY_ID']
STORAGE_SECRET_KEY = os.environ['TODOER_STORAGE_SECRET_KEY']
STORAGE_ENDPOINT = os.environ['TODOER_STORAGE_ENDPOINT']
STORAGE_BUCKET = os.environ['TODOER_STORAGE_BUCKET']

"""After this amount of time, signed url will expire."""
URL_SIGNED_VALIDITY_DURATION_DEFAULT = timedelta(
    minutes=int(
        os.environ['TODOER_STORAGE_PRESIGNED_URL_DURATION_DEFAULT_MINUTES']
    )
)


session = boto3.Session(
    aws_access_key_id=STORAGE_ACCESS_KEY_ID,
    aws_secret_access_key=STORAGE_SECRET_KEY,
)
client = session.client('s3', endpoint_url=STORAGE_ENDPOINT)
s3 = session.resource('s3', endpoint_url=STORAGE_ENDPOINT)
bucket = s3.Bucket(STORAGE_BUCKET)


def put(key: str, file, content_type: str, acl: str = 'public-read'):
    contents = file.read()
    bucket.put_object(
        Key=key, Body=contents, ACL=acl, ContentType=content_type
    )


def delete(key: str):
    client.delete_object(Key=key, Bucket=STORAGE_BUCKET)
    return None


def url_signed_for(key: str, duration: timedelta) -> str:
    return client.generate_presigned_url(
        ClientMethod='get_object',
        Params={
            'Bucket': STORAGE_BUCKET,
            'Key': key,
        },
        ExpiresIn=int(duration.total_seconds()),
    )
