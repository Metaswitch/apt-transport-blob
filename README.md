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

- Environment variables: allows authentication via the mechanisms described in
  [environment_credentials.rs](https://github.com/Azure/azure-sdk-for-rust/blob/main/sdk/identity/src/token_credentials/environment_credentials.rs#L19) - i.e. setting the
  environment variables:
  - `AZURE_TENANT_ID`: The Azure Active Directory tenant/directory ID
  - `AZURE_CLIENT_ID`: The client/application ID of an App Registration in the tenant.
  - `AZURE_CLIENT_SECRET`: A client secret that was generated for the App Registration.
  - or `AZURE_FEDERATED_TOKEN_FILE`: Path to an federated token file.

- Azure CLI credentials: allows authentication via Azure CLI. Log in with
  ```bash
  az login
  ```

- Managed Identity: can be used with Azure VMs, App Service and Azure Functions
  applications.

## Contributing

This project welcomes contributions and suggestions.  Most contributions require you to agree to a
Contributor License Agreement (CLA) declaring that you have the right to, and actually do, grant us
the rights to use your contribution. For details, visit https://cla.opensource.microsoft.com.

When you submit a pull request, a CLA bot will automatically determine whether you need to provide
a CLA and decorate the PR appropriately (e.g., status check, comment). Simply follow the instructions
provided by the bot. You will only need to do this once across all repos using our CLA.

This project has adopted the [Microsoft Open Source Code of Conduct](https://opensource.microsoft.com/codeofconduct/).
For more information see the [Code of Conduct FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or
contact [opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.
