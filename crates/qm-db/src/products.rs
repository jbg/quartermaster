use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

pub const SOURCE_OFF: &str = "openfoodfacts";
pub const SOURCE_MANUAL: &str = "manual";

#[derive(Debug, Clone, Serialize)]
pub struct ProductRow {
    pub id: Uuid,
    pub source: String,
    pub off_barcode: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    pub family: String,
    pub preferred_unit: String,
    pub image_url: Option<String>,
    pub fetched_at: Option<String>,
    pub created_by_household_id: Option<Uuid>,
    pub created_at: String,
}

pub fn base_unit_for_family(family: &str) -> &'static str {
    match family {
        "mass" => "g",
        "volume" => "ml",
        _ => "piece",
    }
}

pub async fn create_manual(
    db: &Database,
    household_id: Uuid,
    name: &str,
    brand: Option<&str>,
    family: &str,
    preferred_unit: Option<&str>,
    barcode: Option<&str>,
) -> Result<ProductRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    let unit = preferred_unit.unwrap_or(base_unit_for_family(family));

    sqlx::query(
        "INSERT INTO product \
         (id, source, off_barcode, name, brand, default_unit, family, image_url, fetched_at, created_by_household_id, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?)",
    )
    .bind(id.to_string())
    .bind(SOURCE_MANUAL)
    .bind(barcode)
    .bind(name)
    .bind(brand)
    .bind(unit)
    .bind(family)
    .bind(household_id.to_string())
    .bind(&created_at)
    .execute(&db.pool)
    .await?;

    Ok(ProductRow {
        id,
        source: SOURCE_MANUAL.to_owned(),
        off_barcode: barcode.map(str::to_owned),
        name: name.to_owned(),
        brand: brand.map(str::to_owned),
        family: family.to_owned(),
        preferred_unit: unit.to_owned(),
        image_url: None,
        fetched_at: None,
        created_by_household_id: Some(household_id),
        created_at,
    })
}

/// Insert or update an OpenFoodFacts-sourced product keyed by its barcode.
pub async fn upsert_from_off(
    db: &Database,
    barcode: &str,
    name: &str,
    brand: Option<&str>,
    family: &str,
    preferred_unit: Option<&str>,
    image_url: Option<&str>,
) -> Result<ProductRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    let unit = preferred_unit.unwrap_or(base_unit_for_family(family));

    if let Some(existing) = find_by_off_barcode(db, barcode).await? {
        sqlx::query(
            "UPDATE product \
             SET name = ?, brand = ?, family = ?, default_unit = ?, image_url = ?, fetched_at = ? \
             WHERE id = ?",
        )
        .bind(name)
        .bind(brand)
        .bind(family)
        .bind(unit)
        .bind(image_url)
        .bind(&now)
        .bind(existing.id.to_string())
        .execute(&db.pool)
        .await?;
        return find_by_id(db, existing.id)
            .await?
            .ok_or(sqlx::Error::RowNotFound);
    }

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO product \
         (id, source, off_barcode, name, brand, default_unit, family, image_url, fetched_at, created_by_household_id, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(id.to_string())
    .bind(SOURCE_OFF)
    .bind(barcode)
    .bind(name)
    .bind(brand)
    .bind(unit)
    .bind(family)
    .bind(image_url)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;

    find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_by_id(db: &Database, id: Uuid) -> Result<Option<ProductRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, source, off_barcode, name, brand, family, default_unit, \
                image_url, fetched_at, created_by_household_id, created_at \
         FROM product WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_product).transpose()
}

pub async fn find_by_off_barcode(
    db: &Database,
    barcode: &str,
) -> Result<Option<ProductRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, source, off_barcode, name, brand, family, default_unit, \
                image_url, fetched_at, created_by_household_id, created_at \
         FROM product WHERE off_barcode = ? AND source = ? AND deleted_at IS NULL",
    )
    .bind(barcode)
    .bind(SOURCE_OFF)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_product).transpose()
}

/// Search products visible to `household_id` — all OFF-sourced products plus
/// this household's manual products. Case-sensitivity follows the DB's default
/// collation; for SQLite this means ASCII case-insensitive via LIKE lowercasing.
pub async fn search(
    db: &Database,
    household_id: Uuid,
    query: &str,
    limit: i64,
) -> Result<Vec<ProductRow>, sqlx::Error> {
    let pattern = format!("%{}%", query.replace('%', r"\%"));
    let rows = sqlx::query(
        "SELECT id, source, off_barcode, name, brand, family, default_unit, \
                image_url, fetched_at, created_by_household_id, created_at \
         FROM product \
         WHERE deleted_at IS NULL \
           AND (source = ? OR created_by_household_id = ?) \
           AND (LOWER(name) LIKE LOWER(?) OR LOWER(COALESCE(brand, '')) LIKE LOWER(?)) \
         ORDER BY name ASC \
         LIMIT ?",
    )
    .bind(SOURCE_OFF)
    .bind(household_id.to_string())
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_product).collect()
}

/// Partial update of a manual product. Pass `Some(Some(value))` to set,
/// `Some(None)` to clear (only applicable to brand/preferred_unit/image_url;
/// the others are required-non-null), or `None` to leave unchanged.
#[derive(Debug, Default, Clone)]
pub struct ProductUpdate<'a> {
    pub name: Option<&'a str>,
    pub brand: Option<Option<&'a str>>,
    pub family: Option<&'a str>,
    pub preferred_unit: Option<&'a str>,
    pub image_url: Option<Option<&'a str>>,
}

pub async fn update(
    db: &Database,
    id: Uuid,
    upd: &ProductUpdate<'_>,
) -> Result<ProductRow, sqlx::Error> {
    let current = find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)?;
    let name = upd.name.unwrap_or(&current.name);
    let family = upd.family.unwrap_or(&current.family);
    let preferred_unit = upd.preferred_unit.unwrap_or(&current.preferred_unit);
    let brand: Option<String> = match upd.brand {
        Some(inner) => inner.map(str::to_owned),
        None => current.brand.clone(),
    };
    let image_url: Option<String> = match upd.image_url {
        Some(inner) => inner.map(str::to_owned),
        None => current.image_url.clone(),
    };

    sqlx::query(
        "UPDATE product SET name = ?, brand = ?, family = ?, default_unit = ?, image_url = ? \
         WHERE id = ?",
    )
    .bind(name)
    .bind(brand.as_deref())
    .bind(family)
    .bind(preferred_unit)
    .bind(image_url.as_deref())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;

    find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Soft-delete a product. The row stays so depleted stock_batches keep
/// resolving their product for history views; finds / searches hide it.
pub async fn soft_delete(db: &Database, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE product SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(now_utc_rfc3339())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Drop the barcode_cache row for this OFF product's barcode. Used by the
/// "Refresh from OpenFoodFacts" endpoint. Returns false if the product
/// isn't OFF-sourced or has no barcode.
pub async fn invalidate_barcode_cache_for(
    db: &Database,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let Some(product) = find_by_id(db, id).await? else {
        return Ok(false);
    };
    let Some(barcode) = product.off_barcode.as_deref() else {
        return Ok(false);
    };
    sqlx::query("DELETE FROM barcode_cache WHERE barcode = ?")
        .bind(barcode)
        .execute(&db.pool)
        .await?;
    Ok(true)
}

fn row_to_product(row: sqlx::any::AnyRow) -> Result<ProductRow, sqlx::Error> {
    let id_str: String = row.try_get("id")?;
    let household_id_str: Option<String> = row.try_get("created_by_household_id")?;
    Ok(ProductRow {
        id: Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        source: row.try_get("source")?,
        off_barcode: row.try_get("off_barcode")?,
        name: row.try_get("name")?,
        brand: row.try_get("brand")?,
        family: row.try_get("family")?,
        preferred_unit: row.try_get("default_unit")?,
        image_url: row.try_get("image_url")?,
        fetched_at: row.try_get("fetched_at")?,
        created_by_household_id: household_id_str
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        created_at: row.try_get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::households;

    #[tokio::test]
    async fn create_and_find_manual_product() {
        let db = crate::test_db().await;
        let h = households::create(&db, "h").await.unwrap();
        let p = create_manual(&db, h.id, "Basmati rice", None, "mass", Some("g"), None)
            .await
            .unwrap();
        assert_eq!(p.family, "mass");
        assert_eq!(p.preferred_unit, "g");
        assert_eq!(p.source, "manual");

        let got = find_by_id(&db, p.id).await.unwrap().unwrap();
        assert_eq!(got.name, "Basmati rice");
    }

    #[tokio::test]
    async fn default_preferred_unit_when_absent() {
        let db = crate::test_db().await;
        let h = households::create(&db, "h").await.unwrap();
        let mass = create_manual(&db, h.id, "Flour", None, "mass", None, None).await.unwrap();
        assert_eq!(mass.preferred_unit, "g");
        let vol = create_manual(&db, h.id, "Milk", None, "volume", None, None).await.unwrap();
        assert_eq!(vol.preferred_unit, "ml");
        let count = create_manual(&db, h.id, "Eggs", None, "count", None, None).await.unwrap();
        assert_eq!(count.preferred_unit, "piece");
    }

    #[tokio::test]
    async fn search_is_household_scoped_for_manuals() {
        let db = crate::test_db().await;
        let a = households::create(&db, "A").await.unwrap();
        let b = households::create(&db, "B").await.unwrap();
        create_manual(&db, a.id, "Alice-Only Product", None, "count", None, None).await.unwrap();
        create_manual(&db, b.id, "Bob-Only Product", None, "count", None, None).await.unwrap();

        let a_results = search(&db, a.id, "only", 10).await.unwrap();
        assert_eq!(a_results.len(), 1);
        assert_eq!(a_results[0].name, "Alice-Only Product");

        let b_results = search(&db, b.id, "only", 10).await.unwrap();
        assert_eq!(b_results.len(), 1);
        assert_eq!(b_results[0].name, "Bob-Only Product");
    }

    #[tokio::test]
    async fn search_sees_off_products_across_households() {
        let db = crate::test_db().await;
        let a = households::create(&db, "A").await.unwrap();
        let b = households::create(&db, "B").await.unwrap();
        upsert_from_off(&db, "5449000000996", "Coca-Cola", Some("Coca-Cola"), "volume", Some("ml"), None)
            .await
            .unwrap();
        assert_eq!(search(&db, a.id, "coca", 10).await.unwrap().len(), 1);
        assert_eq!(search(&db, b.id, "coca", 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn upsert_from_off_updates_existing() {
        let db = crate::test_db().await;
        let first = upsert_from_off(&db, "8076809513388", "Spaghetti", None, "mass", Some("g"), None)
            .await.unwrap();
        let second = upsert_from_off(&db, "8076809513388", "Spaghetti No. 5", Some("Barilla"), "mass", Some("g"), None)
            .await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.name, "Spaghetti No. 5");
        assert_eq!(second.brand.as_deref(), Some("Barilla"));
    }
}
