# apt-transport-blob

A transport which allows installation of Debian packages from Azure Blob Storage.

Implements the APT method interface as documented [here](http://www.fifi.org/doc/libapt-pkg-doc/method.html/ch2.html).

## Building

### Executable

To build the `blob` executable, use `cargo`:

```bash
cargo build --release
```

This creates the `blob` executable in your standard Cargo output directory,
usually `target/release`.

### Debian package

To create a Debian package, use `cargo deb`:

```bash
$ cargo deb
    Finished release [optimized] target(s) in 0.43s
/code/apt-transport-blob/target/debian/apt-transport-blob_<version>_amd64.deb
```

This creates a Debian package in `target/debian`. It contains the `blob`
executable which installs to `/usr/lib/apt/methods/blob`.

## Usage

To use this tool, it needs to be installed in `/usr/lib/apt/methods` as `blob`.
This allows apt to resolve data sources with the `blob://` prefix.

## Authentication

This tool allows several forms of authentication. The user must ensure that
the credential they use is authorised to access the blob container with
the `Storage Blob Data Reader` role.

Credentials are prioritised as follows:

- Storage bearer token: a bearer token created with the `storage.azure.com`
  scope set as the environment variable `AZURE_STORAGE_BEARER_TOKEN`.

  This bearer token can be obtained programmatically in Azure CLI by running
  ```bash
  az account get-access-token --output tsv --query accessToken --resource https://storage.azure.com
  ```

- Workload Identity: for use in environments with federated identity (e.g.
  GitHub Actions, Kubernetes). Requires the following environment variables:
  - `AZURE_TENANT_ID`: The Azure Active Directory tenant/directory ID.
  - `AZURE_CLIENT_ID`: The client/application ID of an App Registration in the tenant.
  - `AZURE_FEDERATED_TOKEN_FILE`: Path to a file containing a federated identity token.

- Client Secret: for use with an App Registration's client secret. Requires the
  following environment variables:
  - `AZURE_TENANT_ID`: The Azure Active Directory tenant/directory ID.
  - `AZURE_CLIENT_ID`: The client/application ID of an App Registration in the tenant.
  - `AZURE_CLIENT_SECRET`: A client secret that was generated for the App Registration.

- Azure CLI: uses an existing Azure CLI login session. Log in with
  ```bash
  az login
  ```

- Managed Identity: for use on Azure compute (VMs, App Service, Azure Functions).
  Uses the Instance Metadata Service (IMDS) to obtain a token. This is tried
  last as it requires a network call that will time out on non-Azure machines.
