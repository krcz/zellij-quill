use crate::error::ApiError;
use clap::Parser;

pub(super) fn parse_args<T: Parser>(args: &[String]) -> Result<T, ApiError> {
    T::try_parse_from(args).map_err(|e| ApiError::new("INVALID_ARGS", e.to_string()))
}
