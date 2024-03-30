use std::{
    array, fmt,
    time::{Duration, SystemTime, SystemTimeError},
};

use crate::image::{cache::StableImage, ImageData};

use http_cache_semantics::CachePolicy;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

pub struct CachePolicyBytes(Vec<u8>);

impl From<&CachePolicy> for CachePolicyBytes {
    fn from(policy: &CachePolicy) -> Self {
        let bytes = bincode::serialize(policy).unwrap();
        Self(bytes)
    }
}

impl TryFrom<&CachePolicyBytes> for CachePolicy {
    type Error = bincode::Error;

    fn try_from(bytes: &CachePolicyBytes) -> Result<Self, Self::Error> {
        let policy = bincode::deserialize(&bytes.0)?;
        Ok(policy)
    }
}

impl ToSql for CachePolicyBytes {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for CachePolicyBytes {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let blob = value.as_blob()?;
        Ok(Self(blob.to_vec()))
    }
}

pub struct SystemTimeSecs(u64);

impl TryFrom<SystemTime> for SystemTimeSecs {
    type Error = SystemTimeError;

    fn try_from(time: SystemTime) -> Result<Self, Self::Error> {
        let since_unix_epoch = time.duration_since(SystemTime::UNIX_EPOCH)?;
        Ok(Self(since_unix_epoch.as_secs()))
    }
}

impl From<SystemTimeSecs> for SystemTime {
    fn from(secs: SystemTimeSecs) -> Self {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs.0)
    }
}

impl ToSql for SystemTimeSecs {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for SystemTimeSecs {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let secs = value.as_i64()?;
        let secs: u64 = secs.try_into().map_err(|_| FromSqlError::InvalidType)?;
        Ok(Self(secs))
    }
}

/// The representation of how a [`StableImage`] is stored in the DB
///
/// The image gets stored as a blob of bytes with a variable-size footer. The footer consists of a
/// byte that indicates the kind of underlying [`StableImage`]. The size of footer depends on the
/// kind of the underlying image
///
/// The reason that we use a footer instead of a header is because it's easier to avoid needlessly
/// copying around the bulky image data if that's what we root the blob around
pub struct StableImageBytes(Vec<u8>);

impl StableImageBytes {
    const COMPRESSED_SVG_KIND: u8 = 0;
    const PRE_DECODED_KIND: u8 = 1;
    // 1 (scale bool) + 8 (2 u32s for dimensions)
    const PRE_DECODED_FOOTER_LEN: usize = 9;

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Clone, Debug)]
pub enum StableImageConvertError {
    MissingKind,
    InvalidKind(u8),
    MissingPreDecodedFooter,
    InvalidPreDecodedScale(u8),
}

impl fmt::Display for StableImageConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKind => f.write_str("Missing stable image kind"),
            Self::InvalidKind(kind) => write!(f, "Invalid stable image kind: {kind}"),
            Self::MissingPreDecodedFooter => f.write_str("Missing pre-decoded image footer"),
            Self::InvalidPreDecodedScale(scale) => write!(f, "Invalid pre-decoded scale: {scale}"),
        }
    }
}

impl std::error::Error for StableImageConvertError {}

impl TryFrom<StableImageBytes> for StableImage {
    type Error = StableImageConvertError;

    fn try_from(bytes: StableImageBytes) -> Result<Self, Self::Error> {
        let mut bytes = bytes.0;
        let kind = bytes.pop().ok_or(StableImageConvertError::MissingKind)?;
        match kind {
            StableImageBytes::COMPRESSED_SVG_KIND => Ok(Self::CompressedSvg(bytes)),
            StableImageBytes::PRE_DECODED_KIND => {
                let footer_start = bytes
                    .len()
                    .checked_sub(StableImageBytes::PRE_DECODED_FOOTER_LEN)
                    .ok_or(StableImageConvertError::MissingPreDecodedFooter)?;
                let (dim_x, dim_y, scale) = {
                    let mut footer = bytes.drain(footer_start..);
                    let scale = match footer.next().expect("Length pre-checked") {
                        0 => false,
                        1 => true,
                        unknown => {
                            return Err(StableImageConvertError::InvalidPreDecodedScale(unknown));
                        }
                    };
                    let dim_x = array::from_fn(|_| footer.next().expect("Length pre-checked"));
                    let dim_y = array::from_fn(|_| footer.next().expect("Length pre-checked"));
                    let dim_x = u32::from_be_bytes(dim_x);
                    let dim_y = u32::from_be_bytes(dim_y);
                    (dim_x, dim_y, scale)
                };
                let image_data = ImageData {
                    lz4_blob: bytes.into(),
                    scale,
                    dimensions: (dim_x, dim_y),
                };
                Ok(Self::PreDecoded(image_data))
            }
            unknown => Err(StableImageConvertError::InvalidKind(unknown)),
        }
    }
}

impl From<StableImage> for StableImageBytes {
    fn from(data: StableImage) -> Self {
        match data {
            StableImage::PreDecoded(ImageData {
                lz4_blob,
                scale,
                dimensions: (dim_x, dim_y),
            }) => {
                let mut bytes = lz4_blob.to_vec();
                bytes.reserve_exact(Self::PRE_DECODED_FOOTER_LEN + 1);
                bytes.push(scale.into());
                bytes.extend_from_slice(&dim_x.to_be_bytes());
                bytes.extend_from_slice(&dim_y.to_be_bytes());
                bytes.push(Self::PRE_DECODED_KIND);
                Self(bytes)
            }
            StableImage::CompressedSvg(mut bytes) => {
                bytes.reserve_exact(1);
                bytes.push(Self::COMPRESSED_SVG_KIND);
                Self(bytes)
            }
        }
    }
}

impl ToSql for StableImageBytes {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for StableImageBytes {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let blob = value.as_blob()?;
        Ok(Self(blob.to_vec()))
    }
}

// TODO: roundtrip prop-test some of ^^. Could try fuzzing with `divan` too since we shouldn't have
// to split out a separate library using that
