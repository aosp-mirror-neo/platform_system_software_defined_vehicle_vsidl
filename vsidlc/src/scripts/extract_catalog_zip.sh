#!/bin/bash
# Copyright (C) 2026 The Android Open Source Project
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set -e

# Arguments from genrule
ZIP_FILE="$1"
shift


# Function to convert a string to snake_case.
# This function mimics the behavior of the heck Rust library's to_snake_case.
to_snake_case() {
  local input_string="$1"
  local snake_case_string

  # First, process camel case by inserting underscores before capital letters,
  # unless they are at the beginning of the string or preceded by an underscore.
  # We use sed to insert an underscore before each uppercase letter that isn't at the start of the string.
  # The 's/([a-z0-9])([A-Z])/\1_\2/g' part adds an underscore between a lowercase letter/digit and an uppercase letter.
  # The 's/([A-Z])([A-Z][a-z])/\1_\2/g' part handles acronyms like 'XML' followed by a word like 'Request'.
  snake_case_string=$(echo "$input_string" | sed -E 's/([a-z0-9])([A-Z])/\1_\2/g' | sed -E 's/([A-Z])([A-Z][a-z])/\1_\2/g')

  # Now convert the entire string to lowercase.
  snake_case_string=$(echo "$snake_case_string" | tr '[:upper:]' '[:lower:]')

  # Finally, replace any remaining non-alphanumeric characters (except underscore) with underscores.
  # This handles spaces, hyphens, and other symbols.
  snake_case_string=$(echo "$snake_case_string" | sed 's/[^a-z0-9_]/_/g')

  # Squeeze multiple underscores into a single one.
  snake_case_string=$(echo "$snake_case_string" | tr -s '_')

  # Remove any leading or trailing underscores.
  snake_case_string=$(echo "$snake_case_string" | sed 's/^_//;s/_$//')

  echo "$snake_case_string"
}

if [ "$1" == "--service-bundle" ]; then
    shift
    PACKAGE_NAME="$1"
    SERVICE_BUNDLE_NAME="$2"

    # --- Script Logic ---

    # Convert package name to path
    PACKAGE_PATH=$(echo "$PACKAGE_NAME" | tr '.' '/')

    # Convert service bundle name to snake_case
    SERVICE_BUNDLE_SNAKE_CASE=$(to_snake_case "$SERVICE_BUNDLE_NAME")

    # Construct the file path inside the zip
    SERVICE_BUNDLE_PATH="generated_rs/service_bundles/${PACKAGE_PATH}/${SERVICE_BUNDLE_SNAKE_CASE}"
    FILE_PATH="${SERVICE_BUNDLE_PATH}/service_bundle.rs"
    DIAG_FILE_PATH="${SERVICE_BUNDLE_PATH}/diagnostics.rs"

    # Extract the file from the zip to correct location
    unzip -p "$ZIP_FILE" "$FILE_PATH" > ./out/service_bundle.rs
    if unzip -l "$ZIP_FILE" | grep -q "$DIAG_FILE_PATH"; then
        # Extract diagnostics.rs only if defined in the zip archive.
        unzip -p "$ZIP_FILE" "$DIAG_FILE_PATH" > ./out/diagnostics.rs
    fi
elif [ "$1" == "--rpc" ]; then
    shift

    if [[ "$#" -eq 3 ]]; then
        # Scenario 1: 3 arguments provided (e.g., package, bundle, unit_type)
        PACKAGE_NAME="$1"
        # SERVICE_BUNDLE_NAME="$2" # This argument is ignored/skipped
        UNIT_TYPE_NAME="$3"
    elif [[ "$#" -eq 2 ]]; then
        # Scenario 2: 2 arguments provided (e.g., package, unit_type)
        PACKAGE_NAME="$1"
        UNIT_TYPE_NAME="$2"
    fi

    # Convert package name to path
    PACKAGE_PATH=$(echo "$PACKAGE_NAME" | tr '.' '/')

    # 1. Extract the package/path part (e.g., 'android.sdv.samples.gateway.cluster')
    PREFIX_DOTTED="${UNIT_TYPE_NAME%.*}"

    # 2. Extract the last part (the class name, e.g., 'MyCluster')
    LAST_PART="${UNIT_TYPE_NAME##*.}"

    # 3. Apply to_snake_case only to the last part
    SNAKE_CASE_LAST_PART=$(to_snake_case "$LAST_PART")

    # 4. Transform dots to slashes in the prefix path
    PREFIX_PATH="${PREFIX_DOTTED//./\/}"

    # 5. Check if the original string contained any dots
    # If the original string had no dots (e.g., 'Cluster'),
    # then PREFIX_DOTTED will be the same as UNIT_TYPE_NAME.
    if [[ "$UNIT_TYPE_NAME" == "$LAST_PART" ]]; then
        # Case: UNIT_TYPE_NAME had NO dots (e.g., 'Cluster')
        PATH_SEGMENT="${SNAKE_CASE_LAST_PART}"
    else
        # Case: UNIT_TYPE_NAME had dots (e.g., 'com.example.Cluster' or 'cluster.cluster')
        # We use the path part, a single slash, and the snake_case part.
        PATH_SEGMENT="${PREFIX_PATH}/${SNAKE_CASE_LAST_PART}"
    fi
    FILE_PATH="generated_rs/rpc/${PACKAGE_PATH}/${PATH_SEGMENT}/lib.rs"

    # Extract the file from the zip to correct location
    unzip -p "$ZIP_FILE" "$FILE_PATH" > ./out/lib.rs
else
    echo "Error: Invalid flag. Use --service-bundle or --rpc." >&2
    exit 1
fi
