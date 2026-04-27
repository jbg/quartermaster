# Changelog

## [1.1.0](https://github.com/jbg/quartermaster/compare/1.0.0...1.1.0) (2026-04-27)


### Features

* add richer reminder context ([e69c0ec](https://github.com/jbg/quartermaster/commit/e69c0ec948f7e807192057a2810624d95f4786fe))
* **android:** add stock correction flow ([4d69c8e](https://github.com/jbg/quartermaster/commit/4d69c8e301963480f98d43aefd2606bbc36c9251))
* **android:** polish detail navigation ([f8a14f7](https://github.com/jbg/quartermaster/commit/f8a14f75932b029be4fb25f1a08a352e8995ebd2))
* **android:** polish route-level daily-driver flows ([fc10f36](https://github.com/jbg/quartermaster/commit/fc10f36b0a2e443bb97ec46cda26d52469f62462))
* **api:** expose stock depletion and location metadata ([2a88048](https://github.com/jbg/quartermaster/commit/2a88048af7d0728332164b6fb35d059638aaca5c))
* expose localizable reminder semantics ([f2f853f](https://github.com/jbg/quartermaster/commit/f2f853f9fd371e185f5aaaf8ccadb112885ff92c))
* group web inventory by location ([e2460d9](https://github.com/jbg/quartermaster/commit/e2460d9160cd18e4588c5e5f1d605904bf1477f9))
* mature push reminder operations ([f4b70b6](https://github.com/jbg/quartermaster/commit/f4b70b6d20154ba5e97050198763dc3fefcca8a3))
* **native:** polish reminder inventory workflows ([4079724](https://github.com/jbg/quartermaster/commit/40797249b080733483f4710cc4d1174a23ca764c))
* **reminders:** enrich expiry reminder copy ([abb8e89](https://github.com/jbg/quartermaster/commit/abb8e892e758ad970a9e3cb94482337796549b53))
* **reminders:** polish reminder inbox actions ([0c3731b](https://github.com/jbg/quartermaster/commit/0c3731be69b40cf7d740ec630ab7ebe0847c8fdf))
* **web:** polish reminder and inventory workflows ([4f42719](https://github.com/jbg/quartermaster/commit/4f427195f0047d1cf15774a35ccc8ced470e472f))
* **web:** use shared unit metadata in inventory flows ([f18d8d8](https://github.com/jbg/quartermaster/commit/f18d8d8a682c185a781dd56fb55b28c14bf17604))


### Bug Fixes

* make smoke fixture seeding idempotent ([7ca6925](https://github.com/jbg/quartermaster/commit/7ca6925188ad29aa37990cd9e84d3c6ca0c1e383))
* satisfy web device registration check ([780eafd](https://github.com/jbg/quartermaster/commit/780eafdaf481e6251d7479b3a3e4f2cce68ee6ea))
* **stock:** reject depleted batch edits ([8a3c6c0](https://github.com/jbg/quartermaster/commit/8a3c6c05b06b8377451b7875dbeb78f33b26aecd))

## [1.0.0](https://github.com/jbg/quartermaster/compare/0.5.0...1.0.0) (2026-04-25)


### ⚠ BREAKING CHANGES

* product and stock PATCH endpoints now accept JSON Patch operation arrays.

### Features

* add Android inventory lifecycle actions ([7346cde](https://github.com/jbg/quartermaster/commit/7346cde14ca592029572468872602a713a049a14))
* add web product management ([068540c](https://github.com/jbg/quartermaster/commit/068540cc136f6c0125307133e9580d62521f4d08))
* **android:** add location management settings ([cd03518](https://github.com/jbg/quartermaster/commit/cd035188b18f00b4371aa9b72b71fd7c3891b164))
* **android:** add product management parity ([fad37e5](https://github.com/jbg/quartermaster/commit/fad37e5dcedb96fb7896e623a93eb78b78483a32))
* use JSON Patch for update requests ([2c4eaf4](https://github.com/jbg/quartermaster/commit/2c4eaf440caf4ac50d2a1629325fe553492b4295))
* **web:** add barcode product entry ([19584fa](https://github.com/jbg/quartermaster/commit/19584fa6e1e8dec8d05e314882ad76f4e8dcf619))

## [0.5.0](https://github.com/jbg/quartermaster/compare/0.4.0...0.5.0) (2026-04-25)


### Features

* move API under versioned prefix ([ccbab99](https://github.com/jbg/quartermaster/commit/ccbab9964f66afe412dd7f2038d2eacfde3df136))

## [0.4.0](https://github.com/jbg/quartermaster/compare/0.3.0...0.4.0) (2026-04-24)


### Features

* add web location management settings ([4160c4e](https://github.com/jbg/quartermaster/commit/4160c4e0c5d43ceef678ab39c2e98782d7626a76))

## [0.3.0](https://github.com/jbg/quartermaster/compare/0.2.0...0.3.0) (2026-04-24)


### Features

* **web:** add stock batch editing ([501ce10](https://github.com/jbg/quartermaster/commit/501ce1077a7012016330314e3f1a93f330a37153))


### Bug Fixes

* allow home assistant add-on data access ([6c182ad](https://github.com/jbg/quartermaster/commit/6c182ad51e494141e5ccfe02fbb75c1b5a5db694))

## [0.2.0](https://github.com/jbg/quartermaster/compare/0.1.2...0.2.0) (2026-04-24)


### Features

* **web:** add product-backed stock creation ([e6bf74a](https://github.com/jbg/quartermaster/commit/e6bf74a55723840a66d2278287a18d16eca71e7a))


### Bug Fixes

* work under HA ingress ([410c564](https://github.com/jbg/quartermaster/commit/410c564094597a4ee77c378d0d374e82a6f60f4d))

## [0.1.2](https://github.com/jbg/quartermaster/compare/0.1.1...0.1.2) (2026-04-24)


### Bug Fixes

* build release images on native arm64 runners ([878cfe8](https://github.com/jbg/quartermaster/commit/878cfe8181e0a5247491ab3b24ca035bc0822e2e))

## [0.1.1](https://github.com/jbg/quartermaster/compare/0.1.0...0.1.1) (2026-04-24)


### Bug Fixes

* configure release-please for workspace versions ([74f26b1](https://github.com/jbg/quartermaster/commit/74f26b1658ae6c945600cfc28dafb5c2efbd3b5b))
* make iOS release config scripts CI-safe ([b9ef683](https://github.com/jbg/quartermaster/commit/b9ef6832666a856422db6815ef2bf67040083137))

## Changelog

Release notes are maintained by Release Please.
