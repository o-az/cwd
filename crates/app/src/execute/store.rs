use {
    super::new_store_code_event,
    crate::{AppError, AppResult, CODES},
    cw_std::{hash, Addr, Binary, Event, Hash, Storage},
    tracing::{info, warn},
};

pub fn store_code(
    store:          &mut dyn Storage,
    uploader:       &Addr,
    wasm_byte_code: &Binary,
) -> AppResult<Vec<Event>> {
    match _store_code(store, uploader, wasm_byte_code) {
        Ok((events, code_hash)) => {
            info!(code_hash = code_hash.to_string(), "Stored code");
            Ok(events)
        },
        Err(err) => {
            warn!(err = err.to_string(), "Failed to store code");
            Err(err)
        },
    }
}

// return the hash of the code that is stored, for purpose of tracing/logging
fn _store_code(
    store:          &mut dyn Storage,
    uploader:       &Addr,
    wasm_byte_code: &Binary,
) -> AppResult<(Vec<Event>, Hash)> {
    // TODO: static check, ensure wasm code has necessary imports/exports
    let code_hash = hash(wasm_byte_code);

    // make sure that the same code isn't uploaded twice
    if CODES.has(store, &code_hash) {
        return Err(AppError::code_exists(code_hash));
    }

    CODES.save(store, &code_hash, wasm_byte_code)?;

    Ok((vec![new_store_code_event(&code_hash, uploader)], code_hash))
}
