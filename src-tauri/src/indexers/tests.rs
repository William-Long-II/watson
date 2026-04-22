#[cfg(test)]
mod tests {
    use crate::indexers::{get_indexer, AppIndexer};

    /// Smoke-only: confirms the platform indexer runs without panicking on
    /// the current machine. It does NOT verify that any specific app was
    /// enumerated, because we have no fixture directory control yet.
    ///
    /// Fixture-backed coverage is tracked as TA-07 in
    /// `docs/testing/test-design-progress.md`. Delete this test once TA-07
    /// lands; until then it is intentionally `#[ignore]`d so `cargo test`
    /// output does not imply coverage we do not actually have.
    #[test]
    #[ignore = "smoke only — real indexer coverage tracked as TA-07"]
    fn indexer_runs_without_panicking_on_host_machine() {
        let indexer = get_indexer();
        let _apps = indexer.index_apps();
    }
}
