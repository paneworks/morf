use crate::{ImageData, ImageError};
use std::path::Path;

/// Published pixels can be inspected without a file or a decoder.
pub(crate) fn source(source: &Path) -> Result<Option<std::sync::Arc<ImageData>>, ImageError> {
    if let Some(name) = source.to_str().and_then(|s| s.strip_prefix("memory:")) {
        return crate::published::published(name)
            .map(Some)
            .ok_or_else(|| ImageError::InvalidSource(source.display().to_string()));
    }
    Ok(None)
}
