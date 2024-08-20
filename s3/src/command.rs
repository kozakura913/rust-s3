use std::collections::HashMap;

use crate::error::S3Error;
use crate::serde_types::{
    BucketLifecycleConfiguration, CompleteMultipartUploadData, CorsConfiguration,
};

use crate::EMPTY_PAYLOAD_SHA;
use sha2::{Digest, Sha256};

pub enum HttpMethod {
    Delete,
    Get,
    Put,
    Post,
    Head,
}

use std::fmt;

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Head => write!(f, "HEAD"),
        }
    }
}
use crate::bucket_ops::BucketConfiguration;
use http::HeaderMap;

#[derive(Clone, Debug)]
pub struct Multipart<'a> {
    part_number: u32,
    upload_id: &'a str,
}

impl<'a> Multipart<'a> {
    pub fn query_string(&self) -> String {
        format!(
            "?partNumber={}&uploadId={}",
            self.part_number, self.upload_id
        )
    }

    pub fn new(part_number: u32, upload_id: &'a str) -> Self {
        Multipart {
            part_number,
            upload_id,
        }
    }
}
#[derive(Default,Clone, Debug)]
pub enum ContentMd5{
    Some(String),
    None,
    #[default]
    Auto,
}
impl From<&[u8]> for ContentMd5{
    fn from(value: &[u8]) -> Self {
        use base64::Engine;
        Self::Some(base64::engine::general_purpose::STANDARD.encode(value))
    }
}
#[derive(Clone, Debug)]
pub enum Command<'a> {
    HeadObject,
    CopyObject {
        from: &'a str,
    },
    DeleteObject,
    DeleteObjectTagging,
    GetObject,
    GetObjectTorrent,
    GetObjectRange {
        start: u64,
        end: Option<u64>,
    },
    GetObjectTagging,
    PutObject {
        content: &'a [u8],
        content_md5: ContentMd5,
        content_type: &'a str,
        multipart: Option<Multipart<'a>>,
        cache_control: Option<&'a str>,
        content_disposition: Option<&'a str>,
    },
    PutObjectTagging {
        tags: &'a str,
    },
    ListMultipartUploads {
        prefix: Option<&'a str>,
        delimiter: Option<&'a str>,
        key_marker: Option<String>,
        max_uploads: Option<usize>,
    },
    ListObjects {
        prefix: String,
        delimiter: Option<String>,
        marker: Option<String>,
        max_keys: Option<usize>,
    },
    ListObjectsV2 {
        prefix: String,
        delimiter: Option<String>,
        continuation_token: Option<String>,
        start_after: Option<String>,
        max_keys: Option<usize>,
    },
    GetBucketLocation,
    PresignGet {
        expiry_secs: u32,
        custom_queries: Option<HashMap<String, String>>,
    },
    PresignPut {
        expiry_secs: u32,
        custom_headers: Option<HeaderMap>,
        custom_queries: Option<HashMap<String, String>>,
    },
    PresignDelete {
        expiry_secs: u32,
    },
    InitiateMultipartUpload {
        content_type: &'a str,
    },
    UploadPart {
        part_number: u32,
        content: &'a [u8],
        content_md5: ContentMd5,
        upload_id: &'a str,
    },
    AbortMultipartUpload {
        upload_id: &'a str,
    },
    CompleteMultipartUpload {
        upload_id: &'a str,
        data: CompleteMultipartUploadData,
        cache_control: Option<&'a str>,
        content_disposition: Option<&'a str>,
    },
    CreateBucket {
        config: BucketConfiguration,
    },
    DeleteBucket,
    ListBuckets,
    PutBucketCors {
        configuration: CorsConfiguration,
    },
    GetBucketLifecycle,
    PutBucketLifecycle {
        configuration: BucketLifecycleConfiguration,
    },
    DeleteBucketLifecycle,
}

impl<'a> Command<'a> {
    pub fn http_verb(&self) -> HttpMethod {
        match *self {
            Command::GetObject
            | Command::GetObjectTorrent
            | Command::GetObjectRange { .. }
            | Command::ListBuckets
            | Command::ListObjects { .. }
            | Command::ListObjectsV2 { .. }
            | Command::GetBucketLocation
            | Command::GetObjectTagging
            | Command::GetBucketLifecycle
            | Command::ListMultipartUploads { .. }
            | Command::PresignGet { .. } => HttpMethod::Get,
            Command::PutObject { .. }
            | Command::CopyObject { from: _ }
            | Command::PutObjectTagging { .. }
            | Command::PresignPut { .. }
            | Command::UploadPart { .. }
            | Command::PutBucketCors { .. }
            | Command::CreateBucket { .. }
            | Command::PutBucketLifecycle { .. } => HttpMethod::Put,
            Command::DeleteObject
            | Command::DeleteObjectTagging
            | Command::AbortMultipartUpload { .. }
            | Command::PresignDelete { .. }
            | Command::DeleteBucket
            | Command::DeleteBucketLifecycle => HttpMethod::Delete,
            Command::InitiateMultipartUpload { .. } | Command::CompleteMultipartUpload { .. } => {
                HttpMethod::Post
            }
            Command::HeadObject => HttpMethod::Head,
        }
    }

    pub fn content_length(&self) -> Result<usize, S3Error> {
        let result = match &self {
            Command::CopyObject { from: _ } => 0,
            Command::PutObject { content, .. } => content.len(),
            Command::PutObjectTagging { tags } => tags.len(),
            Command::UploadPart { content, .. } => content.len(),
            Command::CompleteMultipartUpload { data, .. } => data.len(),
            Command::CreateBucket { config } => {
                if let Some(payload) = config.location_constraint_payload() {
                    Vec::from(payload).len()
                } else {
                    0
                }
            }
            Command::PutBucketLifecycle { configuration } => {
                quick_xml::se::to_string(configuration)?.as_bytes().len()
            }
            _ => 0,
        };
        Ok(result)
    }

    pub fn content_type(&self) -> String {
        match self {
            Command::InitiateMultipartUpload { content_type } => content_type.to_string(),
            Command::PutObject { content_type, .. } => content_type.to_string(),
            Command::CompleteMultipartUpload { .. } | Command::PutBucketLifecycle { .. } => {
                "application/xml".into()
            }
            _ => "text/plain".into(),
        }
    }

    pub fn sha256(&self) -> Result<String, S3Error> {
        let result = match &self {
            Command::PutObject { content, .. } => {
                let mut sha = Sha256::default();
                sha.update(content);
                hex::encode(sha.finalize().as_slice())
            }
            Command::PutObjectTagging { tags } => {
                let mut sha = Sha256::default();
                sha.update(tags.as_bytes());
                hex::encode(sha.finalize().as_slice())
            }
            Command::CompleteMultipartUpload { data, .. } => {
                let mut sha = Sha256::default();
                sha.update(data.to_string().as_bytes());
                hex::encode(sha.finalize().as_slice())
            }
            Command::CreateBucket { config } => {
                if let Some(payload) = config.location_constraint_payload() {
                    let mut sha = Sha256::default();
                    sha.update(payload.as_bytes());
                    hex::encode(sha.finalize().as_slice())
                } else {
                    EMPTY_PAYLOAD_SHA.into()
                }
            }
            _ => EMPTY_PAYLOAD_SHA.into(),
        };
        Ok(result)
    }
}
