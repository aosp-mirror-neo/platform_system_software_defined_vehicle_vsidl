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

# SDV VSIDL Provider Samples

## VSIDL Provider Cross VM Query

Sample Cross VM VSIDL Provider Call.

The optional "loop_period" and "num_loops" arguments allow a looped version of
the VSIDL Provider Call.

### Build

```bash
  cd path/to/android-repo
  source build/envsetup.sh
  lunch sdv_core_cf-trunk_staging-userdebug
  m
```

### Test VSIDL Provider (cross-VM)

1.  Launch 1st SDV VM

    ```bash
    cvd create
    ```

2.  Launch 2nd SDV VM

    ```bash
    cvd create --config=sdv_core_instance2
    ```

3.  Start a new terminal and run on 1st VM

    ```bash
    adb -s 0.0.0.0:6520 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name DiagService --query-variant diagnostics-declaration
    adb -s 0.0.0.0:6520 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name Manager --query-variant publication-descriptor --unit-type-name TirePressure
    adb -s 0.0.0.0:6520 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name HelloService --query-variant rpc-method-descriptor --unit-type-name HelloInterface --method-name SayHello
    adb -s 0.0.0.0:6520 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name Manager --query-variant message-descriptor --message-name com.android.sdv.sample.vsidl_provider.TirePressure
    ```

4.  Verify the printed logs on instance1

    ```bash
    adb -s 0.0.0.0:6520 logcat | grep "Querying VSIDL Provider"
    # Expect log "Querying VSIDL Provider Diagnostics from VM instance1 to VM instance2. Received response: Some(_ /* Diagnostics Data */)"
    # Expect log "Querying VSIDL Provider Publication Descriptor from VM instance1 to VM instance2. Received response: Some(_ /* PublicationDescriptor */)"
    # Expect log "Querying VSIDL Provider RPC Method Descriptor from VM instance1 to VM instance2. Received response: Some(_ /* RpcMethodDescriptor */)"
    # Expect log "Querying VSIDL Provider Message Descriptor from VM instance1 to VM instance2. Received response: Some(_ /* MessageDescriptor */)"
    ```

5.  Start a new terminal and run on 2nd VM

    ```bash
    adb -s 0.0.0.0:6521 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name DiagService --query-variant diagnostics-declaration
    adb -s 0.0.0.0:6521 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name Manager --query-variant publication-descriptor --unit-type-name TirePressure
    adb -s 0.0.0.0:6521 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name HelloService --query-variant rpc-method-descriptor --unit-type-name HelloInterface --method-name SayHello
    adb -s 0.0.0.0:6521 shell su root sdv_vsidl_provider_query --server-vm-name instance2 --package-name com.android.sdv.sample.vsidl_provider --bundle-name Manager --query-variant message-descriptor --message-name com.android.sdv.sample.vsidl_provider.TirePressure
    ```

6.  Verify the printed logs on instance2

    ```bash
    adb -s 0.0.0.0:6521 logcat | grep "Querying VSIDL Provider"
    # Expect log "Querying VSIDL Provider Diagnostics from VM instance2 to VM instance2. Received response: Some(_ /* Diagnostics Data */)"
    # Expect log "Querying VSIDL Provider Publication Descriptor from VM instance2 to VM instance2. Received response: Some(_ /* PublicationDescriptor */)"
    # Expect log "Querying VSIDL Provider RPC Method Descriptor from VM instance2 to VM instance2. Received response: Some(_ /* RpcMethodDescriptor */)"
    # Expect log "Querying VSIDL Provider Message Descriptor from VM instance2 to VM instance2. Received response: Some(_ /* MessageDescriptor */)"
    ```
