# 3. Backend

## 3.1 Language & framework

- **Rust** for all backend code.
- **Axum** as the HTTP framework.
- Cargo workspace; each plugin is a member crate.

## 3.2 Plugin contract (backend side)

Each plugin crate implements the `Plugin` trait on a top-level struct. The trait exposes a `routes(&self, resources: PluginResources) -> Router` method that builds an Axum router scoped to the plugin's resources, plus lifecycle and job hooks. Full interface specification in [11-backend-plugin-interface.md](11-backend-plugin-interface.md).

`junius` ([07-junius.md](07-junius.md)) generates a `plugins.rs` that constructs and registers each enabled plugin in order. Plugin dependency order is resolved from manifests (see [06-plugin-shape.md](06-plugin-shape.md)).

## 3.3 API contract: Connect-RPC

- Each plugin owns one or more `.proto` files defining its services.
- `connect-rs` generates the Rust server stubs.
- `@connectrpc/connect-web` (+ `@bufbuild/protoc-gen-es`) generates the TS client.
- RPC routes mount under `/rpc/<plugin-name>/*`.
- Schema evolution is enforced by Buf's `buf breaking` checks in CI.

Connect-RPC was chosen over OpenAPI and rspc for:
- Strongest schema evolution discipline.
- Best long-term ecosystem stability (Buf is a serious company with a real product roadmap).
- First-class TS client with great DX (works over plain HTTP, no gRPC plumbing).
