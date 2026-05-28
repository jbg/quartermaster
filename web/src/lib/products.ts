import {
  ApiFailure,
  type Product,
  type Unit,
  type UnitFamily,
  type UpdateProductRequest
} from './session-core';
import { productPreferredUnit, unitChoicesForFamily } from './inventory';

export type ProductIncludeFilter = 'active' | 'all' | 'deleted';

export interface ProductFormFields {
  name: string;
  brand: string;
  family: UnitFamily;
  preferredUnit: string;
  imageUrl: string;
  maxOpenDays: string;
  packageQuantity?: string;
  packageUnit?: string;
}

export function productBrand(product: Product): string {
  return product.brand?.trim() ?? '';
}

export function productImageUrl(product: Product): string {
  return product.image_url ?? product.imageUrl ?? '';
}

export function productBarcode(product: Product): string {
  return product.barcode ?? '';
}

export function productDeletedAt(product: Product): string {
  return product.deleted_at ?? product.deletedAt ?? '';
}

export function productMaxOpenDays(product: Product): number | null {
  return product.max_open_days ?? product.maxOpenDays ?? null;
}

export function productPackageQuantity(product: Product): string {
  return String(product.package_quantity ?? product.packageQuantity ?? '');
}

export function productPackageUnit(product: Product): string {
  return (
    product.package_unit ??
    product.packageUnit ??
    (product.family === 'volume' ? 'ml' : product.family === 'mass' ? 'g' : 'piece')
  );
}

export function isDeletedProduct(product: Product): boolean {
  return productDeletedAt(product).length > 0;
}

export function isManualProduct(product: Product): boolean {
  return product.source !== 'openfoodfacts';
}

export function productSourceLabel(product: Product): string {
  return product.source === 'openfoodfacts' ? 'OpenFoodFacts' : 'Manual';
}

export function emptyProductForm(units: Unit[] = []): ProductFormFields {
  return {
    name: '',
    brand: '',
    family: 'mass',
    preferredUnit: unitChoicesForFamily('mass', units)[0],
    imageUrl: '',
    maxOpenDays: '',
    packageQuantity: '',
    packageUnit: unitChoicesForFamily('mass', units)[0]
  };
}

export function productFormFields(product: Product, units: Unit[] = []): ProductFormFields {
  return {
    name: product.name,
    brand: productBrand(product),
    family: product.family,
    preferredUnit: productPreferredUnit(product, units),
    imageUrl: productImageUrl(product),
    maxOpenDays: productMaxOpenDays(product)?.toString() ?? '',
    packageQuantity: productPackageQuantity(product),
    packageUnit: productPackageUnit(product)
  };
}

export function validateProductForm(fields: ProductFormFields, units: Unit[] = []): string | null {
  if (!fields.name.trim()) {
    return 'Enter a product name.';
  }
  if (fields.name.trim().length > 256) {
    return 'Product name must be 256 characters or fewer.';
  }
  if (!unitChoicesForFamily(fields.family, units).includes(fields.preferredUnit)) {
    return 'Choose a preferred unit that matches the product family.';
  }
  if (fields.maxOpenDays.trim()) {
    const days = Number(fields.maxOpenDays.trim());
    if (!Number.isInteger(days) || days <= 0) {
      return 'Maximum open days must be a positive whole number.';
    }
  }
  return null;
}

export function setProductFormFamily(
  fields: ProductFormFields,
  family: UnitFamily,
  units: Unit[] = []
): ProductFormFields {
  const unitChoices = unitChoicesForFamily(family, units);
  const packageUnit =
    fields.packageUnit && unitChoices.includes(fields.packageUnit)
      ? fields.packageUnit
      : unitChoices[0];
  return {
    ...fields,
    family,
    preferredUnit: unitChoices[0],
    packageUnit
  };
}

export function buildProductCreateRequest(fields: ProductFormFields) {
  return {
    name: fields.name.trim(),
    brand: fields.brand.trim() || null,
    family: fields.family,
    preferred_unit: fields.preferredUnit,
    barcode: null,
    image_url: fields.imageUrl.trim() || null,
    max_open_days: fields.maxOpenDays.trim() ? Number(fields.maxOpenDays.trim()) : null
  };
}

export function buildProductUpdateRequest(
  product: Product,
  fields: ProductFormFields
): UpdateProductRequest {
  const request: UpdateProductRequest = [];
  const name = fields.name.trim();
  if (name !== product.name) {
    request.push({ op: 'replace', path: '/name', value: name });
  }
  if (fields.family !== product.family) {
    request.push({ op: 'replace', path: '/family', value: fields.family });
  }
  if (fields.preferredUnit !== productPreferredUnit(product)) {
    request.push({ op: 'replace', path: '/preferred_unit', value: fields.preferredUnit });
  }
  applyClearableText(request, 'brand', productBrand(product), fields.brand);
  applyClearableText(request, 'image_url', productImageUrl(product), fields.imageUrl);
  applyClearableNumber(request, 'max_open_days', productMaxOpenDays(product), fields.maxOpenDays);
  applyPackageSize(request, product, fields);
  return request;
}

function applyPackageSize(
  request: UpdateProductRequest,
  product: Product,
  fields: ProductFormFields
) {
  const currentQuantity = productPackageQuantity(product);
  const currentUnit = product.package_unit ?? product.packageUnit ?? '';
  const nextQuantity = (fields.packageQuantity ?? '').trim();
  const nextUnit = (fields.packageUnit ?? productPackageUnit(product)).trim();
  if (nextQuantity) {
    if (nextQuantity !== currentQuantity) {
      request.push({ op: 'replace', path: '/package_quantity', value: nextQuantity });
    }
    if (nextUnit !== currentUnit) {
      request.push({ op: 'replace', path: '/package_unit', value: nextUnit });
    }
  } else if (currentQuantity || currentUnit) {
    request.push({ op: 'remove', path: '/package_quantity' });
    request.push({ op: 'remove', path: '/package_unit' });
  }
}

function applyClearableText(
  request: UpdateProductRequest,
  key: 'brand' | 'image_url' | 'package_quantity' | 'package_unit',
  currentValue: string,
  nextValue: string
) {
  const trimmed = nextValue.trim();
  if (trimmed) {
    if (trimmed !== currentValue) {
      request.push({ op: 'replace', path: `/${key}`, value: trimmed });
    }
  } else if (currentValue) {
    request.push({ op: 'remove', path: `/${key}` });
  }
}

function applyClearableNumber(
  request: UpdateProductRequest,
  key: 'max_open_days',
  currentValue: number | null,
  nextValue: string
) {
  const trimmed = nextValue.trim();
  if (trimmed) {
    const parsed = Number(trimmed);
    if (parsed !== currentValue) {
      request.push({ op: 'replace', path: `/${key}`, value: parsed });
    }
  } else if (currentValue !== null) {
    request.push({ op: 'remove', path: `/${key}` });
  }
}

export function parseProductInclude(value: string | null): ProductIncludeFilter {
  if (value === 'all' || value === 'deleted') {
    return value;
  }
  return 'active';
}

export function includeDeletedForFilter(filter: ProductIncludeFilter): boolean {
  return filter !== 'active';
}

export function filterDeletedProducts(
  products: Product[],
  filter: ProductIncludeFilter
): Product[] {
  if (filter === 'deleted') {
    return products.filter(isDeletedProduct);
  }
  return products;
}

export function productListHref(query: string, include: ProductIncludeFilter): string {
  const params = new URLSearchParams();
  if (query.trim()) {
    params.set('q', query.trim());
  }
  if (include !== 'active') {
    params.set('include', include);
  }
  const suffix = params.toString();
  return suffix ? `/products?${suffix}` : '/products';
}

export function productMutationErrorMessage(err: unknown, fallback: string): string {
  if (!(err instanceof ApiFailure)) {
    return fallback;
  }
  switch (err.code) {
    case 'off_product_read_only':
      return 'Only local OpenFoodFacts corrections can be saved here.';
    case 'off_credentials_not_configured':
      return 'OpenFoodFacts contribution is not configured on this server.';
    case 'off_credentials_missing':
      return 'Save your OpenFoodFacts credentials in Settings first.';
    case 'off_contribution_no_changes':
      return 'There are no local OpenFoodFacts corrections to contribute.';
    case 'off_authentication_failed':
      return 'OpenFoodFacts rejected the saved credentials.';
    case 'product_has_stock':
      return 'This product still has active stock. Consume or discard it first.';
    case 'product_has_incompatible_stock':
      return 'This product has active stock with units that do not fit the new family.';
    case 'unit_family_mismatch':
    case 'unknown_unit':
      return 'Choose a unit that matches the product family.';
    case 'not_found':
      return 'Product could not be found.';
    default:
      return err.message || fallback;
  }
}

export function barcodeLookupErrorMessage(err: unknown): string {
  if (!(err instanceof ApiFailure)) {
    return 'Barcode lookup failed.';
  }
  switch (err.status) {
    case 400:
      return 'Enter an EAN-8, UPC-A, EAN-13, or EAN-14 barcode.';
    case 404:
      return 'No product was found for that barcode.';
    case 429:
      return 'Barcode lookup is rate-limited. Try again shortly.';
    case 502:
      return 'Barcode lookup is temporarily unavailable.';
    default:
      return err.message || 'Barcode lookup failed.';
  }
}
