//! Capsule resolution at new-submission and accepted-recovery boundaries.

use crate::error::Result;
use crate::protocol::TaskRequest;

pub(super) fn resolve(request: &mut TaskRequest, already_accepted: bool) -> Result<()> {
    request.validate()?;
    if already_accepted {
        crate::capsule::ensure_request_bound(request)?;
    } else {
        crate::capsule::resolve_external_request(request)?;
        request.validate()?;
    }
    Ok(())
}
