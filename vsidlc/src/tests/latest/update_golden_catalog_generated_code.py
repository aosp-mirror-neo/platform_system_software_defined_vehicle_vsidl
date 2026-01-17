#!/usr/bin/env python3
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

# Script to update VSIDL golden files.

import os
import shutil
import subprocess
from pathlib import Path

# Ensure ANDROID_BUILD_TOP is set and resolve its absolute path.
ANDROID_BUILD_TOP_ENV_VAR = os.getenv("ANDROID_BUILD_TOP")
if not ANDROID_BUILD_TOP_ENV_VAR:
    print(
        "Error: ANDROID_BUILD_TOP environment variable is not set. "
        "Please run `source build/envsetup.sh` and select a target."
    )
    sys.exit(1)

ANDROID_BUILD_TOP = os.path.abspath(ANDROID_BUILD_TOP_ENV_VAR)
VSIDLC_BASE_DIR = os.path.join(
    ANDROID_BUILD_TOP, 'system', 'software_defined_vehicle', 'vsidl',
    'vsidlc', 'src', 'tests')
VSIDLC_GOLDEN_DIR = os.path.join(VSIDLC_BASE_DIR, 'latest')
VSIDLC_CATALOGS_DIR = os.path.join(VSIDLC_BASE_DIR, 'catalogs')

# Constant for vsidlc output directory
VSIDLC_OUTPUT_DIR = os.path.join(VSIDLC_GOLDEN_DIR, 'output')


def list_files_in(directory):
        """Finds all files in dir and subdirs.

        Args:
        directory: Base directory to explore.
        relative: If True, returns paths relative to base dir.

        Returns:
        Sorted list of Paths.
        """
        return sorted(
            [
                Path(path, name)
                for path, _, files in os.walk(directory)
                for name in files
            ]
        )

def rename_files(directory):
    """Renames files in a directory (and subdirectories), changing the extension."""
    for file in list_files_in(directory):
        if file.suffix == ".bp":
            file.rename(file.with_suffix("._bp"))
        if file.suffix == ".rs":
            file.rename(file.with_suffix("._rs"))

def process_directory(catalog_path, dependencies=None, output_dir=VSIDLC_OUTPUT_DIR, genrule=False):
    """Processes a given catalog directory using vsidlc.

    Args:
        catalog_path: Path to the main catalog directory.
        dependencies: (Optional) A list of paths to additional dependency catalogs.  Defaults to None.
        output_dir: (Optional) Directory where the generated code will be placed. Defaults to VSIDLC_OUTPUT_DIR.
        genrule: (Optional) Flag that runs vsidlc with --genrule flag when set.
    """
    print(f"Processing catalog: {catalog_path}")
    os.makedirs(output_dir, exist_ok=True)

    cmd = ["vsidlc", "--catalog-path", catalog_path, "--output-path", output_dir, "--rust-formatter", "pretty-please", "--purge", "--target-api", "latest"]

    if dependencies:
        for dep in dependencies:
            cmd.extend(["--dependency-catalog-path", dep])

    if genrule:
        cmd.append("--genrule")

    try:
        subprocess.run(cmd, check=True, capture_output=True, text=True)
    except subprocess.CalledProcessError as e:
        print(f"Error: vsidlc command failed for catalog '{catalog_path}'. Check paths and dependencies.")
        print(f"vsidlc stdout:\n{e.stdout}")
        print(f"vsidlc stderr:\n{e.stderr}")
        return 1

    rename_files(output_dir)
    return 0

if __name__ == "__main__":
    for item in os.listdir(VSIDLC_CATALOGS_DIR):
        path = os.path.join(VSIDLC_CATALOGS_DIR, item)
        if os.path.isdir(path) and item.startswith("catalog"):
            dependencies = None
            if item == "catalog_without_androidbp":
                # this catalog is different because is not self-contained and does not have Android.bp
                # catalog_without_androidbp relies on messages defined in the ./catalog folder
                # therefore we need to add ./catalog as dependency for this catalog to properly build.
                dependencies = [os.path.join(VSIDLC_CATALOGS_DIR, 'catalog')]
            if process_directory(path, dependencies=dependencies) != 0:
                exit(1)

            output_rs_dir = os.path.join(VSIDLC_GOLDEN_DIR, f"golden_{item}_rs")
            if os.path.exists(output_rs_dir):
                shutil.rmtree(output_rs_dir)
            shutil.move(os.path.join(VSIDLC_OUTPUT_DIR, "generated_rs"), output_rs_dir)
            shutil.rmtree(VSIDLC_OUTPUT_DIR)

            # execute vsidlc with --android-bp flag only for catalog
            if item != "catalog":
                continue
            if process_directory(path, dependencies=dependencies, genrule=True) != 0:
                exit(1)
            output_rs_dir = os.path.join(VSIDLC_GOLDEN_DIR, f"genrule_golden/{item}")
            if os.path.exists(output_rs_dir):
                shutil.rmtree(output_rs_dir)
            shutil.move(os.path.join(VSIDLC_OUTPUT_DIR, "generated_rs"), output_rs_dir)
            shutil.rmtree(VSIDLC_OUTPUT_DIR)


    print("All catalogs updated")
