//! The desktop protocols, beside a window client on its connection.

use morf_app::LayerClient;
use morf_desktop::Desktop;

/// The desktop protocols on `client`'s connection; a request naming no
/// output is for the one `client`'s surface sits on.
pub(crate) fn desktop_for(client: &LayerClient) -> Result<Desktop, String> {
    let mut desktop = Desktop::new(client.connection())?;
    desktop.set_own_output(client.own_output().and_then(|output| output.name));
    Ok(desktop)
}
