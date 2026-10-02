//! Values read from 2xx responses of source requests, by request and by
//! the fingerprint of the resolved request that was sent. The fingerprint
//! includes environment values, so each environment keeps its own value.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::model::CheckSource;

/// Entries kept per request, newest first.
const ENTRIES_PER_REQUEST: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueKey {
    pub source: CheckSource,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    pub request_id: u64,
    pub fingerprint: String,
    pub fetched_at_ms: u64,
    pub values: Vec<(ValueKey, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResponseTokenCache {
    /// Newest first.
    entries: Vec<CacheEntry>,
}

impl ResponseTokenCache {
    pub fn record(
        &mut self,
        request_id: u64,
        fingerprint: &str,
        fetched_at_ms: u64,
        values: Vec<(ValueKey, String)>,
    ) {
        self.entries
            .retain(|e| !(e.request_id == request_id && e.fingerprint == fingerprint));
        self.entries.insert(
            0,
            CacheEntry {
                request_id,
                fingerprint: fingerprint.to_string(),
                fetched_at_ms,
                values,
            },
        );
        let mut kept = 0;
        self.entries.retain(|e| {
            if e.request_id != request_id {
                return true;
            }
            kept += 1;
            kept <= ENTRIES_PER_REQUEST
        });
    }

    pub fn lookup(
        &self,
        request_id: u64,
        fingerprint: &str,
        key: &ValueKey,
    ) -> Option<(&str, u64)> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.request_id == request_id && e.fingerprint == fingerprint)?;
        entry
            .values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| (value.as_str(), entry.fetched_at_ms))
    }

    pub fn prune(&mut self, live_ids: &HashSet<u64>) {
        self.entries.retain(|e| live_ids.contains(&e.request_id));
    }

    pub fn entries(&self) -> &[CacheEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CheckSource;

    fn key(path: &str) -> ValueKey {
        ValueKey {
            source: CheckSource::Json,
            path: path.into(),
        }
    }

    #[test]
    fn record_replaces_the_same_fingerprint_and_keeps_others() {
        let mut cache = ResponseTokenCache::default();
        cache.record(1, "dev", 10, vec![(key(".t"), "a".into())]);
        cache.record(1, "prod", 20, vec![(key(".t"), "b".into())]);
        cache.record(1, "dev", 30, vec![(key(".t"), "c".into())]);
        assert_eq!(cache.lookup(1, "dev", &key(".t")), Some(("c", 30)));
        assert_eq!(cache.lookup(1, "prod", &key(".t")), Some(("b", 20)));
        assert_eq!(cache.lookup(1, "dev", &key(".other")), None);
        assert_eq!(cache.lookup(2, "dev", &key(".t")), None);
    }

    #[test]
    fn keeps_the_newest_eight_entries_per_request() {
        let mut cache = ResponseTokenCache::default();
        for n in 0..10u64 {
            cache.record(1, &format!("f{n}"), n, vec![]);
        }
        cache.record(2, "x", 0, vec![]);
        assert_eq!(
            cache.entries().iter().filter(|e| e.request_id == 1).count(),
            8
        );
        assert!(
            cache
                .entries()
                .iter()
                .all(|e| e.fingerprint != "f0" && e.fingerprint != "f1")
        );
        assert_eq!(
            cache.entries().iter().filter(|e| e.request_id == 2).count(),
            1
        );
    }

    #[test]
    fn prune_drops_entries_of_missing_requests() {
        let mut cache = ResponseTokenCache::default();
        cache.record(1, "a", 0, vec![]);
        cache.record(2, "a", 0, vec![]);
        cache.prune(&[2].into_iter().collect());
        assert_eq!(cache.entries().len(), 1);
        assert_eq!(cache.entries()[0].request_id, 2);
    }

    #[test]
    fn round_trips_through_json() {
        let mut cache = ResponseTokenCache::default();
        cache.record(1, "a", 5, vec![(key(".t"), "v".into())]);
        let text = serde_json::to_string(&cache).unwrap();
        let back: ResponseTokenCache = serde_json::from_str(&text).unwrap();
        assert_eq!(back, cache);
    }
}
