import { describe, expect, it } from 'vitest';
import { ApiFailure, type Product } from './session-core';
import {
  barcodeLookupErrorMessage,
  buildProductCreateRequest,
  buildProductUpdateRequest,
  filterDeletedProducts,
  includeDeletedForFilter,
  parseProductInclude,
  productListHref,
  productMutationErrorMessage,
  setProductFormFamily,
  validateProductForm
} from './products';

describe('product helpers', () => {
  it('validates product form fields', () => {
    expect(
      validateProductForm({
        name: '',
        brand: '',
        family: 'mass',
        preferredUnit: 'g',
        imageUrl: '',
        maxOpenDays: ''
      })
    ).toBe('Enter a product name.');
    expect(
      validateProductForm({
        name: 'Milk',
        brand: '',
        family: 'volume',
        preferredUnit: 'kg',
        imageUrl: '',
        maxOpenDays: ''
      })
    ).toBe('Choose a preferred unit that matches the product family.');
    expect(
      validateProductForm({
        name: 'Milk',
        brand: '',
        family: 'volume',
        preferredUnit: 'l',
        imageUrl: '',
        maxOpenDays: '0'
      })
    ).toBe('Maximum open days must be a positive whole number.');
    expect(
      validateProductForm({
        name: 'Milk',
        brand: '',
        family: 'volume',
        preferredUnit: 'l',
        imageUrl: '',
        maxOpenDays: '3'
      })
    ).toBeNull();
  });

  it('builds create payloads with trimmed optional fields', () => {
    expect(
      buildProductCreateRequest({
        name: '  Rice  ',
        brand: '  House  ',
        family: 'mass',
        preferredUnit: 'kg',
        imageUrl: '  https://example.test/rice.jpg  ',
        maxOpenDays: '5'
      })
    ).toEqual({
      name: 'Rice',
      brand: 'House',
      family: 'mass',
      preferred_unit: 'kg',
      barcode: null,
      image_url: 'https://example.test/rice.jpg',
      max_open_days: 5
    });
  });

  it('builds update payloads and explicitly clears empty optional text', () => {
    const product: Product = {
      id: 'product-1',
      name: 'Rice',
      brand: 'House',
      family: 'mass',
      preferred_unit: 'kg',
      image_url: 'https://example.test/rice.jpg',
      max_open_days: 7
    };

    expect(
      buildProductUpdateRequest(product, {
        name: 'Rice Long Grain',
        brand: '',
        family: 'mass',
        preferredUnit: 'g',
        imageUrl: '',
        maxOpenDays: ''
      })
    ).toEqual([
      { op: 'replace', path: '/name', value: 'Rice Long Grain' },
      { op: 'replace', path: '/preferred_unit', value: 'g' },
      { op: 'remove', path: '/brand' },
      { op: 'remove', path: '/image_url' },
      { op: 'remove', path: '/max_open_days' }
    ]);
  });

  it('resets package unit when changing to a different unit family', () => {
    expect(
      setProductFormFamily(
        {
          name: 'Olive oil',
          brand: '',
          family: 'count',
          preferredUnit: 'piece',
          imageUrl: '',
          maxOpenDays: '',
          packageQuantity: '5',
          packageUnit: 'piece'
        },
        'volume'
      )
    ).toMatchObject({
      family: 'volume',
      preferredUnit: 'ml',
      packageQuantity: '5',
      packageUnit: 'ml'
    });
  });

  it('builds OFF local correction payloads for product family and package size', () => {
    const product: Product = {
      id: 'product-1',
      name: 'Olive oil',
      family: 'count',
      preferred_unit: 'piece',
      package_quantity: null,
      package_unit: null,
      source: 'openfoodfacts'
    };

    expect(
      buildProductUpdateRequest(product, {
        name: 'Olive oil',
        brand: '',
        family: 'volume',
        preferredUnit: 'ml',
        imageUrl: '',
        maxOpenDays: '',
        packageQuantity: '5000',
        packageUnit: 'ml'
      })
    ).toEqual([
      { op: 'replace', path: '/family', value: 'volume' },
      { op: 'replace', path: '/preferred_unit', value: 'ml' },
      { op: 'replace', path: '/package_quantity', value: '5000' },
      { op: 'replace', path: '/package_unit', value: 'ml' }
    ]);
  });

  it('parses and serializes list filters', () => {
    expect(parseProductInclude(null)).toBe('active');
    expect(parseProductInclude('all')).toBe('all');
    expect(parseProductInclude('deleted')).toBe('deleted');
    expect(includeDeletedForFilter('active')).toBe(false);
    expect(includeDeletedForFilter('all')).toBe(true);
    expect(productListHref(' rice ', 'deleted')).toBe('/products?q=rice&include=deleted');
  });

  it('filters deleted-only rows client-side after requesting tombstones', () => {
    const active: Product = { id: 'a', name: 'Active', family: 'count' };
    const deleted: Product = {
      id: 'd',
      name: 'Deleted',
      family: 'count',
      deleted_at: '2026-04-25T00:00:00Z'
    };
    expect(filterDeletedProducts([active, deleted], 'deleted')).toEqual([deleted]);
    expect(filterDeletedProducts([active, deleted], 'all')).toEqual([active, deleted]);
  });

  it('maps product API failures to user-facing messages', () => {
    expect(
      productMutationErrorMessage(
        new ApiFailure(409, 'server text', 'product_has_stock'),
        'Fallback'
      )
    ).toBe('This product still has active stock. Consume or discard it first.');
    expect(
      productMutationErrorMessage(
        new ApiFailure(403, 'server text', 'off_product_read_only'),
        'Fallback'
      )
    ).toBe('Only local OpenFoodFacts corrections can be saved here.');
    expect(productMutationErrorMessage(new Error('nope'), 'Fallback')).toBe('Fallback');
  });

  it('maps barcode lookup API failures to user-facing messages', () => {
    expect(barcodeLookupErrorMessage(new ApiFailure(400))).toBe(
      'Enter an EAN-8, UPC-A, EAN-13, or EAN-14 barcode.'
    );
    expect(barcodeLookupErrorMessage(new ApiFailure(404))).toBe(
      'No product was found for that barcode.'
    );
    expect(barcodeLookupErrorMessage(new ApiFailure(429))).toBe(
      'Barcode lookup is rate-limited. Try again shortly.'
    );
    expect(barcodeLookupErrorMessage(new ApiFailure(502))).toBe(
      'Barcode lookup is temporarily unavailable.'
    );
    expect(barcodeLookupErrorMessage(new Error('nope'))).toBe('Barcode lookup failed.');
    expect(barcodeLookupErrorMessage(new ApiFailure(500, 'Unexpected barcode failure'))).toBe(
      'Unexpected barcode failure'
    );
  });
});
