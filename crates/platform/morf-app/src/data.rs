//! What a drag, a drop or a selection carries, before and after it is read.

/// One thing on offer — a selection or a drag — before any of it is read.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OfferInfo {
    /// Client-local identifier, unique for the life of the connection, that
    /// [`crate::backend::wayland::LayerClient::read_offer`] names the offer by.
    pub id: u64,
    /// Every type the source can produce, in the source's own order.
    pub mime_types: Vec<String>,
}

/// A drop, with the parts nearly every target wants already fetched.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DropInfo {
    /// The offer, still readable until the drop is finished.
    pub offer: OfferInfo,
    /// The type this client accepted for the drop.
    pub accepted: Option<String>,
    /// Parsed `text/uri-list`, when the source offered one.
    pub uris: Vec<String>,
    /// The best text type, decoded, when the source offered one.
    pub text: Option<String>,
}
