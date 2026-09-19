# AI App Builder

The desktop App Builder turns a natural-language requirement into an untrusted candidate App Definition, validates it with the Rust Runtime and presents the accepted definition for review. It never executes model-generated code.

## Configure a provider

Open **Provider setup** and enter a profile ID, display name, provider type, API base URL and model. Use **OpenAI-compatible** for a hosted HTTPS API and **Local compatible server** for an HTTPS or loopback HTTP service such as `http://127.0.0.1:11434/v1`. A full `/chat/completions` URL is also accepted. Hosted profiles require an API key; local profiles may omit it.

Profile metadata is stored in the AIX SQLite database. API keys are sent directly to the operating-system credential store and are never written to the profile database or App Definition. Leaving the key input empty preserves an existing key.

## Generate, revise and run

1. Select a provider profile and describe the application.
2. Choose **Generate definition**. AIX sends the requirement, public schema and registered Operation contracts to the selected endpoint.
3. Invalid output receives at most two validator-driven repair attempts. Only a definition accepted by the Rust Runtime reaches the review screen.
4. Review the JSON Pointer change list and requested capabilities. Enter a follow-up requirement and choose **Revise definition** to modify the accepted result.
5. Choose **Review permissions and run**. Runtime permission grants remain a separate explicit step.

The provider call has a 60-second timeout, a 1 MiB response limit and no redirects. The selected profile fixes the request destination. Unencrypted local endpoints must resolve from a literal loopback host; arbitrary HTTP hosts are rejected.

## Export

Choose **Save JSON** or **Save YAML** after review. The host revalidates the definition immediately before writing it. Files use the name `<app-id>.aix.json` or `<app-id>.aix.yaml` under `Downloads/AIX` where a Downloads directory is available. The desktop UI displays the exact saved path.
