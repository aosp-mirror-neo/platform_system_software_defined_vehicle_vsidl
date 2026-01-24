<!--
  Copyright (C) 2026 The Android Open Source Project

  Licensed under the Apache License, Version 2.0 (the "License");
  you may not use this file except in compliance with the License.
  You may obtain a copy of the License at

      http://www.apache.org/licenses/LICENSE-2.0

  Unless required by applicable law or agreed to in writing, software
  distributed under the License is distributed on an "AS IS" BASIS,
  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
  See the License for the specific language governing permissions and
  limitations under the License.
-->

# VSIDL Golden File Testing

## Concept

Golden tests protect the `vsidlc` from unexpected changes to its output. They
are 'end-to-end', meaning the test inputs are `.vsidl` and `.proto` files:

-   A set of samples test catalog (./catalog*), is run through the latest build
    of `vsidlc`.
-   The output is then compared to the **'golden reference files'**
    golden_catalog*.

The golden files represent the test specification. Hence, changes to the golden
files must be [updated](#update-golden-files), reviewed and submitted
'manually'.

Valid changes to the golden files include:

-   extension of the test catalog.
-   deliberate changes to the generator output.

## Local Testing & Updating

The tests are executed as part of the regular build process and can be run via:

```bash
atest sdv_core_vsidl_unit_vsidlc_golden_test_rs
```

The test runner is `golden_tests.rs`, which compares the output of `vsidlc` with
the golden files.

### Update Golden Files

To update the golden files, run the `update_golden_catalog_generated_code.py`
script:

```bash
./update_golden_catalog_generated_code.py
```

This script will regenerate the golden files based on the current output of
`vsidlc`.

### Appendix

#### File extension

All file defined in the `golden_catalog*/` folders do not have the original
extension, but have been updated with _{ext}. For example:

-   Android.bp files are stored as Android._bp to avoid been conflicts with
    other target names
-   rust_files.rs are mapped to rust_files._rs to avoid git hook and linter to
    update them using `rustfmt`, since those files are formatted using
    `prettyplease`.

When the test generates files from the given catalogs, the files are updated to
follow the same extension scheme, in this way it's possible to easily compare
generated code and golden files.
