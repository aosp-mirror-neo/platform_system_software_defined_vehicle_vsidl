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
# VSIDLC_PATH [mandatory] - path to the vsidlc tool within the sandbox
# SOONG_ZIP_PATH [mandatory] - path to soong_zip tool within the sandbox
# CATALOG_FILES [mandatory] - array of all files belonging to the catalog
# GEN_DIR [mandatory] - output folder used by vsidlc tool as output within the sandbox
# OUTPUT_ZIP [mandatory] - output zip file name
# DEPENDENCY_FILES [optional] - array of filegroups linked as dependencies
# --
# VSIDLC_ARGS [optional] - additional arguments provided to vsidlc tool

# --- Argument Parsing ---
ARGS=()
VSIDLC_ARGS=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --)
      shift
      VSIDLC_ARGS="$*"
      break
      ;;
    *)
      ARGS+=("$1")
      shift
      ;;
  esac
done

VSIDLC_PATH="${ARGS[0]}"
SOONG_ZIP_PATH="${ARGS[1]}"
CATALOG_FILES="${ARGS[2]}"
GEN_DIR="${ARGS[3]}"
OUTPUT_ZIP="${ARGS[4]}"
DEP_FILES="${ARGS[5]}" # This can be empty if not provided

echo "vsidlc extra arguments $VSIDLC_ARGS"
# --- Script Logic ---

# Move vsidl annotation in local path
cp -r ./system/software_defined_vehicle/vsidl/language/src/protos/* .

# Input catalog files
echo "input catalog: $CATALOG_FILES"
echo "input dependencies: $DEP_FILES"

# Replace spaces with newlines to prepare for sorting
# The -z flag handles the null-delimited output of sort, which is safer
SORTED_FILES=$(echo "$CATALOG_FILES" | tr ' ' '\n' | sort -z)
echo "input catalog: $SORTED_FILES"

# Get the first and last file from the sorted list
FIRST_FILE=$(echo "$SORTED_FILES" | head -n 1)
LAST_FILE=$(echo "$SORTED_FILES" | tail -n 1)

# Find the common prefix character by character
LONGEST_COMMON_PATH=""
for (( i=0; i<${#FIRST_FILE}; i++ )); do
    # Break if the characters don't match
    if [ "${FIRST_FILE:i:1}" != "${LAST_FILE:i:1}" ]; then
        break
    fi
    # Append the character to the common path
    LONGEST_COMMON_PATH+="${FIRST_FILE:i:1}"
done

# Trim the path to the last full directory
# This handles cases where the common prefix ends in a partial filename
CATALOG_DIR=$(echo "$LONGEST_COMMON_PATH" | sed 's/\/[^/]*$//')

# Create dependency flags from dependency files
DEP_DIRS=""
for dep_file in $DEP_FILES; do
  dep_dir=$(dirname "$dep_file")
  DEP_DIRS="$DEP_DIRS $dep_dir"
done

UNIQUE_DEP_DIRS=$(echo "$DEP_DIRS" | tr ' ' '\n' | sort -u)

# Filter out subdirectories. Since the list of directories is sorted,
# a subdirectory will always appear after its parent. We only need to
# check if a directory is a child of the last top-level directory we found.
FINAL_DEP_DIRS=()
last_top_level_dir=""
while IFS= read -r dir; do
  if [ -z "$dir" ]; then
    continue
  fi

  # If last_top_level_dir is empty or dir is not a child of it...
  if [[ -z "$last_top_level_dir" ]] || [[ ! "$dir" == "$last_top_level_dir"/* ]]; then
    # ...then we've found a new top-level directory.
    FINAL_DEP_DIRS+=("$dir")
    last_top_level_dir="$dir"
  fi
done <<< "$UNIQUE_DEP_DIRS"

DEP_FLAGS=()
for dir in "${FINAL_DEP_DIRS[@]}"; do
    # Check if dir is a sub-directory of CATALOG_DIR, is CATALOG_DIR
    # itself, or an ancestor of CATALOG_DIR.
    if [[ "$dir" != "$CATALOG_DIR" && "$dir" != "$CATALOG_DIR"/* && "$CATALOG_DIR" != "$dir"/* ]]; then
        DEP_FLAGS+=(--dependency-catalog-path "$dir")
    fi
done

echo "dependency flags: ${DEP_FLAGS[@]}"

echo "--- Running VSIDL Generation ---"
echo "Catalog Directory: $CATALOG_DIR"
echo "Output Directory: $GEN_DIR"
echo "Running: $VSIDLC_PATH -c $CATALOG_DIR -o $GEN_DIR --rust-formatter pretty-please ${DEP_FLAGS[@]} $VSIDLC_ARGS"

# Run the VSIDL compiler
"$VSIDLC_PATH" -c "$CATALOG_DIR" -o "$GEN_DIR" --rust-formatter pretty-please "${DEP_FLAGS[@]}" $VSIDLC_ARGS

echo "--- Zipping Generated Files ---"
# Zip the results
"$SOONG_ZIP_PATH" -o "$OUTPUT_ZIP" -C "$GEN_DIR" -D "$GEN_DIR/generated_rs"

echo "--- Genrule Script Completed ---"