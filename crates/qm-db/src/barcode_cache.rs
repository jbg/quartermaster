use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub barcode: String,
    pub product_id: Option<Uuid>,
    pub fetched_at: String,
    pub miss: bool,
}

impl CacheEntry {
    pub fn is_fresh(
        &self,
        now: DateTime<Utc>,
        positive_ttl_days: i64,
        negative_ttl_days: i64,
    ) -> bool {
        let Ok(fetched) = DateTime::parse_from_rfc3339(&self.fetched_at) else {
            return false;
        };
        let fetched_utc = fetched.with_timezone(&Utc);
        let ttl_days = if self.miss {
            negative_ttl_days
        } else {
            positive_ttl_days
        };
        (now - fetched_utc).num_days() < ttl_days
    }
}

pub async fn get(db: &Database, barcode: &str) -> Result<Option<CacheEntry>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT barcode, product_id, fetched_at, miss FROM barcode_cache WHERE barcode = ?",
    )
    .bind(barcode)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_entry).transpose()
}

pub async fn put_hit(db: &Database, barcode: &str, product_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM barcode_cache WHERE barcode = ?")
        .bind(barcode)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO barcode_cache (barcode, product_id, raw_off_json, fetched_at, miss) \
         VALUES (?, ?, NULL, ?, 0)",
    )
    .bind(barcode)
    .bind(product_id.to_string())
    .bind(now_utc_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn put_miss(db: &Database, barcode: &str) -> Result<(), sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM barcode_cache WHERE barcode = ?")
        .bind(barcode)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO barcode_cache (barcode, product_id, raw_off_json, fetched_at, miss) \
         VALUES (?, NULL, NULL, ?, 1)",
    )
    .bind(barcode)
    .bind(now_utc_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

fn row_to_entry(row: sqlx::any::AnyRow) -> Result<CacheEntry, sqlx::Error> {
    let product_id_str: Option<String> = row.try_get("product_id")?;
    let miss: i64 = row.try_get("miss")?;
    Ok(CacheEntry {
        barcode: row.try_get("barcode")?,
        product_id: product_id_str
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        fetched_at: row.try_get("fetched_at")?,
        miss: miss != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, products};

    #[tokio::test]
    async fn hit_then_lookup() {
        let db = crate::test_db().await;
        let h = households::create(&db, "h").await.unwrap();
        let p = products::create_manual(&db, h.id, "Test", None, "count", None, None, None)
            .await
            .unwrap();
        put_hit(&db, "1234567890123", p.id).await.unwrap();
        let got = get(&db, "1234567890123").await.unwrap().unwrap();
        assert!(!got.miss);
        assert_eq!(got.product_id, Some(p.id));
    }

    #[tokio::test]
    async fn miss_then_lookup() {
        let db = crate::test_db().await;
        put_miss(&db, "0000000000000").await.unwrap();
        let got = get(&db, "0000000000000").await.unwrap().unwrap();
        assert!(got.miss);
        assert!(got.product_id.is_none());
    }

    #[tokio::test]
    async fn miss_overwrites_hit() {
        let db = crate::test_db().await;
        let h = households::create(&db, "h").await.unwrap();
        let p = products::create_manual(&db, h.id, "Test", None, "count", None, None, None)
            .await
            .unwrap();
        put_hit(&db, "1111111111111", p.id).await.unwrap();
        put_miss(&db, "1111111111111").await.unwrap();
        let got = get(&db, "1111111111111").await.unwrap().unwrap();
        assert!(got.miss);
    }

    #[test]
    fn freshness_check() {
        let now = Utc::now();
        let fresh = CacheEntry {
            barcode: "x".into(),
            product_id: None,
            fetched_at: (now - chrono::Duration::days(1)).to_rfc3339(),
            miss: false,
        };
        assert!(fresh.is_fresh(now, 30, 7));

        let stale = CacheEntry {
            barcode: "x".into(),
            product_id: None,
            fetched_at: (now - chrono::Duration::days(31)).to_rfc3339(),
            miss: false,
        };
        assert!(!stale.is_fresh(now, 30, 7));

        let miss_fresh = CacheEntry {
            barcode: "x".into(),
            product_id: None,
            fetched_at: (now - chrono::Duration::days(3)).to_rfc3339(),
            miss: true,
        };
        assert!(miss_fresh.is_fresh(now, 30, 7));

        let miss_stale = CacheEntry {
            barcode: "x".into(),
            product_id: None,
            fetched_at: (now - chrono::Duration::days(8)).to_rfc3339(),
            miss: true,
        };
        assert!(!miss_stale.is_fresh(now, 30, 7));
    }
}
