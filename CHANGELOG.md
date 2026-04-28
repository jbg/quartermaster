# Changelog

## [1.6.0](https://github.com/jbg/quartermaster/compare/1.5.0...1.6.0) (2026-04-28)


### Features

* **android:** add in-app setup QR scanning ([ab507c0](https://github.com/jbg/quartermaster/commit/ab507c04e11c69ffa228ebcdcdf92e4ca81fa67f))
* **api:** add recovery email verification ([bdfcc17](https://github.com/jbg/quartermaster/commit/bdfcc1751deca97166ab334d7de83dc094cbff31))
* **api:** expose auth capability descriptors ([ecc1455](https://github.com/jbg/quartermaster/commit/ecc14550de9b3216c632e14c51055f3af0bb1d75))
* **native:** add recovery email settings ([c7d0def](https://github.com/jbg/quartermaster/commit/c7d0defe399538bdc98d4b55296c27b118d7f1a3))


### Bug Fixes

* **web:** align onboarding auth method type ([4f60d34](https://github.com/jbg/quartermaster/commit/4f60d34ade71a6e4ebdfc12358f76041f4a774e2))
* **web:** recover auth under Home Assistant ingress ([24bcaed](https://github.com/jbg/quartermaster/commit/24bcaedacd56146b2ffd59559d29e2bdf00bf709))

## [1.5.0](https://github.com/jbg/quartermaster/compare/1.4.1...1.5.0) (2026-04-28)


### Features

* add onboarding setup endpoints ([fed2eac](https://github.com/jbg/quartermaster/commit/fed2eac6434bb8b6ed6ab4ff22cf5627740daaae))
* streamline Android onboarding ([a2c89b0](https://github.com/jbg/quartermaster/commit/a2c89b030756c553adbaab901839c7fe49238d51))
* streamline iOS onboarding ([a3cca5e](https://github.com/jbg/quartermaster/commit/a3cca5e7199d3a93a533b3618026fe3cc578e5d6))
* update web onboarding links ([47cb152](https://github.com/jbg/quartermaster/commit/47cb152eef534cce572d1fd44877aeeb29bb8fe6))

## [1.4.1](https://github.com/jbg/quartermaster/compare/1.4.0...1.4.1) (2026-04-28)


### Bug Fixes

* repair TestFlight release deploy ([19d6e49](https://github.com/jbg/quartermaster/commit/19d6e490b62ceca76dc68242c7568847b8114f2f))
* select jsonwebtoken crypto provider ([1fd9030](https://github.com/jbg/quartermaster/commit/1fd90303ff852b2d249c72833b0f359de26cfb63))
* **web:** update generated client for dependency bump ([fe1c1ee](https://github.com/jbg/quartermaster/commit/fe1c1eefe883182c14e2d8b1c2eeec0bb5feb3a0))

## [1.4.0](https://github.com/jbg/quartermaster/compare/1.3.4...1.4.0) (2026-04-28)


### Features

* add server url mobile pairing ([9f0ba99](https://github.com/jbg/quartermaster/commit/9f0ba998080b7d380872e4c36077255dc4c58ad9))


### Bug Fixes

* **web:** respect ingress paths in the hosted shell ([b9247c0](https://github.com/jbg/quartermaster/commit/b9247c0a4322a2eabb092704c2cf64fc5b724f54))

## [1.3.4](https://github.com/jbg/quartermaster/compare/1.3.3...1.3.4) (2026-04-28)


### Bug Fixes

* require iOS 26 SDK for TestFlight uploads ([fafddb6](https://github.com/jbg/quartermaster/commit/fafddb67db274b8e9780b19c2841dcf7da1d0251))

## [1.3.3](https://github.com/jbg/quartermaster/compare/1.3.2...1.3.3) (2026-04-28)


### Bug Fixes

* make iOS app icons opaque ([382b0a4](https://github.com/jbg/quartermaster/commit/382b0a455d2dab8d8d285a4263e891cc8c8dbebe))

## [1.3.2](https://github.com/jbg/quartermaster/compare/1.3.1...1.3.2) (2026-04-27)


### Bug Fixes

* resolve iOS notification settings concurrency ([0d61775](https://github.com/jbg/quartermaster/commit/0d617755bd9e8ce02a3abb01fd00b72da626fb15))

## [1.3.1](https://github.com/jbg/quartermaster/compare/1.3.0...1.3.1) (2026-04-27)


### Bug Fixes

* use async notification settings API ([33989d2](https://github.com/jbg/quartermaster/commit/33989d2a5aa66b3bac3bbe75461244c79c6b1df5))

## [1.3.0](https://github.com/jbg/quartermaster/compare/1.2.6...1.3.0) (2026-04-27)


### Features

* add Quartermaster brand identity assets ([f52d060](https://github.com/jbg/quartermaster/commit/f52d060afcf4c7f0feb3862d9397d13df07ccbb6))

## [1.2.6](https://github.com/jbg/quartermaster/compare/1.2.5...1.2.6) (2026-04-27)


### Bug Fixes

* add iOS app store metadata ([87fb98f](https://github.com/jbg/quartermaster/commit/87fb98fa53a645ab3a4c3c93e1acc9690d1824d0))

## [1.2.5](https://github.com/jbg/quartermaster/compare/1.2.4...1.2.5) (2026-04-27)


### Bug Fixes

* resolve release signing from generated config ([168f7cc](https://github.com/jbg/quartermaster/commit/168f7cc155b971d744b9bdd9d07c2d1afffc52c1))

## [1.2.4](https://github.com/jbg/quartermaster/compare/1.2.3...1.2.4) (2026-04-27)


### Bug Fixes

* move iOS archive signing into release config ([5be6877](https://github.com/jbg/quartermaster/commit/5be68772124c23638a88ecc0e4d31ff1db0a30e2))

## [1.2.3](https://github.com/jbg/quartermaster/compare/1.2.2...1.2.3) (2026-04-27)


### Bug Fixes

* scope iOS archive provisioning to device builds ([882f215](https://github.com/jbg/quartermaster/commit/882f215a227e54df0a1079699a2a845f4b66c455))

## [1.2.2](https://github.com/jbg/quartermaster/compare/1.2.1...1.2.2) (2026-04-27)


### Bug Fixes

* use distribution identity for iOS archives ([93be6a7](https://github.com/jbg/quartermaster/commit/93be6a75c6ecedff38ee8c86dff977ff8f2dbd9b))

## [1.2.1](https://github.com/jbg/quartermaster/compare/1.2.0...1.2.1) (2026-04-27)


### Bug Fixes

* pass downloaded iOS provisioning profile to archive ([8750940](https://github.com/jbg/quartermaster/commit/8750940ba0217f0b59e768dbe2be4071ac0aee29))

## [1.2.0](https://github.com/jbg/quartermaster/compare/1.1.1...1.2.0) (2026-04-27)


### Features

* add cookie-backed web auth ([8e2b97c](https://github.com/jbg/quartermaster/commit/8e2b97cdc586991bb004cbb15addcfcac8efc82f))
* localize reminder client copy ([ad5f60a](https://github.com/jbg/quartermaster/commit/ad5f60a2db435690237978bf1cf9937582fa9117))


### Bug Fixes

* expose csrf cookie to web routes ([515b651](https://github.com/jbg/quartermaster/commit/515b651310b45bdd3222992c6a8b843aa8be675e))

## [1.1.1](https://github.com/jbg/quartermaster/compare/1.1.0...1.1.1) (2026-04-27)


### Bug Fixes

* clarify Xcode managed signing profiles ([6d0669f](https://github.com/jbg/quartermaster/commit/6d0669f5af1b4842bb5ec958fde1161c3eb41bb1))
* support local iOS provisioning profiles ([c2762a9](https://github.com/jbg/quartermaster/commit/c2762a92deb446b897636b973b1d63c2de80331f))

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
