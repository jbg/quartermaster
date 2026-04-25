import {
  ApiFailure,
  type Product,
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

export function isDeletedProduct(product: Product): boolean {
  return productDeletedAt(product).length > 0;
}

export function isManualProduct(product: Product): boolean {
  return product.source !== 'openfoodfacts';
}

export function productSourceLabel(product: Product): string {
  return product.source === 'openfoodfacts' ? 'OpenFoodFacts' : 'Manual';
}

export function emptyProductForm(): ProductFormFields {
  return {
    name: '',
    brand: '',
    family: 'mass',
    preferredUnit: 'g',
    imageUrl: ''
  };
}

export function productFormFields(product: Product): ProductFormFields {
  return {
    name: product.name,
    brand: productBrand(product),
    family: product.family,
    preferredUnit: productPreferredUnit(product),
    imageUrl: productImageUrl(product)
  };
}

export function validateProductForm(fields: ProductFormFields): string | null {
  if (!fields.name.trim()) {
    return 'Enter a product name.';
  }
  if (fields.name.trim().length > 256) {
    return 'Product name must be 256 characters or fewer.';
  }
  if (!unitChoicesForFamily(fields.family).includes(fields.preferredUnit)) {
    return 'Choose a preferred unit that matches the product family.';
  }
  return null;
}

export function setProductFormFamily(
  fields: ProductFormFields,
  family: UnitFamily
): ProductFormFields {
  return {
    ...fields,
    family,
    preferredUnit: unitChoicesForFamily(family)[0]
  };
}

export function buildProductCreateRequest(fields: ProductFormFields) {
  return {
    name: fields.name.trim(),
    brand: fields.brand.trim() || null,
    family: fields.family,
    preferred_unit: fields.preferredUnit,
    barcode: null,
    image_url: fields.imageUrl.trim() || null
  };
}

export function buildProductUpdateRequest(
  product: Product,
  fields: ProductFormFields
): UpdateProductRequest {
  const request: UpdateProductRequest = {};
  const name = fields.name.trim();
  if (name !== product.name) {
    request.name = name;
  }
  if (fields.family !== product.family) {
    request.family = fields.family;
  }
  if (fields.preferredUnit !== productPreferredUnit(product)) {
    request.preferred_unit = fields.preferredUnit;
  }
  applyClearableText(request, 'brand', productBrand(product), fields.brand);
  applyClearableText(request, 'image_url', productImageUrl(product), fields.imageUrl);
  return request;
}

function applyClearableText(
  request: UpdateProductRequest,
  key: 'brand' | 'image_url',
  currentValue: string,
  nextValue: string
) {
  const trimmed = nextValue.trim();
  if (trimmed) {
    if (trimmed !== currentValue) {
      request[key] = trimmed;
    }
  } else if (currentValue) {
    request[key] = null;
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
      return 'OpenFoodFacts products are read-only from the web catalogue.';
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
