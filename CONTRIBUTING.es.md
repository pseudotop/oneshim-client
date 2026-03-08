# Guía de Contribución del Cliente Rust de ONESHIM

Gracias por su interés en el cliente Rust de ONESHIM. Este documento es la guía específica de Rust para contribuir al workspace de Cargo de 10 crates.

## Configuración del Entorno de Desarrollo

### Requisitos Previos

- **Rust** 1.77.1 o posterior (mantenga actualizado con `rustup update stable`)
- **cargo** — Sistema de compilación y gestor de paquetes de Rust (incluido con Rust)
- **pnpm** — Requerido para compilar el panel web del frontend (`oneshim-web/frontend`)

### Configuración

```bash
# 1. Clonar el repositorio
git clone https://github.com/pseudotop/oneshim-client.git
cd oneshim-client

# 2. Verificar dependencias y compilar
cargo check --workspace

# 3. Compilar el frontend (si incluye el panel web)
cd crates/oneshim-web/frontend
pnpm install
pnpm build
cd ../../..

# 4. Compilación completa
cargo build --workspace
```

### Características Opcionales

Algunas características se controlan mediante feature flags.

```bash
# Habilitar OCR (requiere Tesseract)
cargo build -p oneshim-vision --features ocr

# Habilitar cliente gRPC (tonic/prost)
cargo build -p oneshim-network --features grpc
```

## Compilación

### Compilación de Desarrollo

```bash
# Verificación rápida del workspace
cargo check --workspace

# Compilación de desarrollo
cargo build -p oneshim-app

# Ejecutar en modo de desarrollo
cargo run -p oneshim-app
```

### Compilación con Frontend

El panel web embebe la salida de compilación de React en el binario de Rust.

```bash
# Paso 1: Compilar el frontend
cd crates/oneshim-web/frontend && pnpm install && pnpm build
# O use el script
./scripts/build-frontend.sh

# Paso 2: Compilar el binario de Rust (embebe dist/ automáticamente)
cargo build --release -p oneshim-app
```

### Compilación Completa del Workspace

```bash
# Compilación de lanzamiento para todos los crates
cargo build --release --workspace
```

### Compilar Crates Específicos

```bash
cargo build -p oneshim-core
cargo build -p oneshim-network
cargo build -p oneshim-vision
```

## Estilo de Código

### Formato

Todo el código sigue la configuración predeterminada de `cargo fmt`. Ejecútelo antes de enviar un PR.

```bash
# Aplicar formato
cargo fmt --all

# Verificar formato (igual que en CI)
cargo fmt --check
```

### Lint

`cargo clippy` debe reportar 0 advertencias. Si necesita suprimir una advertencia, agregue `#[allow(...)]` al elemento específico y explique el motivo en un comentario.

```bash
# Ejecutar clippy en el workspace completo
cargo clippy --workspace

# Ejecutar con todas las características habilitadas
cargo clippy --workspace --all-features
```

### Comentarios y Documentación

- **Los comentarios de código y docstrings deben escribirse en inglés de forma predeterminada.**
- **La documentación pública es principalmente en inglés, con documentos complementarios en coreano para las guías clave.**
- Agregue comentarios de documentación `///` a todos los elementos `pub`.
- Use comentarios en línea (`//`) para explicar la intención en lógica compleja.
- Para la gobernanza de documentación, siga [docs/DOCUMENTATION_POLICY.md](./docs/DOCUMENTATION_POLICY.md).
- Política complementaria en coreano: [docs/DOCUMENTATION_POLICY.ko.md](./docs/DOCUMENTATION_POLICY.ko.md)
- Para métricas de calidad mutables, actualice únicamente [docs/STATUS.md](./docs/STATUS.md).

```rust
/// Screen capture trigger — decides whether to capture based on event importance.
pub struct SmartCaptureTrigger {
    // Timestamp of last capture — used for throttling
    last_capture: Instant,
}
```

### Manejo de Errores

- Crates de biblioteca: use `thiserror` para definir enums de error concretos
- Crate binario (`oneshim-app`): use `anyhow::Result`
- Envuelva errores de crates externos con `#[from]`

```rust
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// No auth token available
    #[error("no auth token")]
    NoToken,
}
```

### Traits Asíncronos

Aplique `#[async_trait]` a todos los traits de port. Esto es necesario para el patrón de DI con `Arc<dyn PortTrait>`.

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ApiClient: Send + Sync {
    /// Uploads a context payload to the server.
    async fn upload_context(&self, context: &ContextPayload) -> Result<(), CoreError>;
}
```

## Reglas de Arquitectura

Este proyecto sigue estrictamente **Hexagonal Architecture (Ports & Adapters)**. Por favor, comprenda estas reglas antes de contribuir.

### Principio Fundamental

**`oneshim-core` define todos los traits de port y los modelos de dominio.** Los otros 9 crates son adapters.

```
oneshim-core  (definiciones de port, modelos)
    <- oneshim-monitor   (adapter de monitoreo del sistema)
    <- oneshim-vision    (adapter de procesamiento de imagen)
    <- oneshim-network   (adapter HTTP/SSE/WebSocket)
    <- oneshim-storage   (adapter SQLite)
    <- oneshim-suggestion <- oneshim-network
    <- src-tauri          <- oneshim-suggestion
    <- oneshim-automation
    <- oneshim-app        (cableado DI completo)
```

### Patrones Prohibidos

Las dependencias directas entre crates adapter no están permitidas. Por ejemplo, `oneshim-monitor` no debe depender directamente de `oneshim-storage`. Toda la comunicación entre crates se realiza a través de traits definidos en `oneshim-core`.

Excepciones permitidas:
- `oneshim-suggestion` -> `oneshim-network` (recepción SSE)
- `src-tauri` -> `oneshim-suggestion` (visualización de sugerencias)

### Patrón de DI

Use inyección por constructor con `Arc<dyn T>`. No se utiliza ningún framework de DI; todo el cableado se realiza manualmente en `oneshim-app/src/main.rs`.

```rust
pub struct Scheduler {
    // Dependencies injected via Arc<dyn T> pattern
    monitor: Arc<dyn SystemMonitor>,
    storage: Arc<dyn StorageService>,
    api_client: Arc<dyn ApiClient>,
}

impl Scheduler {
    pub fn new(
        monitor: Arc<dyn SystemMonitor>,
        storage: Arc<dyn StorageService>,
        api_client: Arc<dyn ApiClient>,
    ) -> Self {
        Self { monitor, storage, api_client }
    }
}
```

## Agregar Nuevas Características

Siga este orden al agregar nueva funcionalidad.

### Paso 1: Definir un Port en core

Agregue un nuevo trait en `crates/oneshim-core/src/ports/`.

```rust
// crates/oneshim-core/src/ports/my_service.rs

use async_trait::async_trait;
use crate::error::CoreError;

/// Port interface for the new feature
#[async_trait]
pub trait MyService: Send + Sync {
    /// Performs the operation.
    async fn do_something(&self, input: &str) -> Result<String, CoreError>;
}
```

### Paso 2: Implementar el Adapter

Implemente el trait en el crate adapter correspondiente.

```rust
// crates/oneshim-xxx/src/my_impl.rs

use async_trait::async_trait;
use oneshim_core::{ports::MyService, error::CoreError};

pub struct MyServiceImpl {
    // Fields needed for the implementation
}

#[async_trait]
impl MyService for MyServiceImpl {
    async fn do_something(&self, input: &str) -> Result<String, CoreError> {
        // Actual implementation
        todo!()
    }
}
```

### Paso 3: Conectar el DI en app

Conecte la implementación a su port en `crates/oneshim-app/src/main.rs`.

```rust
// crates/oneshim-app/src/main.rs

let my_service: Arc<dyn MyService> = Arc::new(MyServiceImpl::new());
let scheduler = Scheduler::new(my_service, /* other dependencies */);
```

### Paso 4: Escribir Pruebas

Escriba tanto pruebas unitarias como pruebas de integración.

```rust
// Unit tests: place at the bottom of the relevant module
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_do_something() {
        let svc = MyServiceImpl::new();
        let result = svc.do_something("input").await;
        assert!(result.is_ok());
    }
}
```

## Escritura de Pruebas

### Principios

- **No use mockall.** Escriba los mocks manualmente.
- Coloque las pruebas en un bloque `#[cfg(test)] mod tests` al final de cada módulo.
- Implemente los traits de port directamente para crear mocks de prueba.

### Patrón de Mock Manual

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::ports::ApiClient;

    // Test mock — only defined inside the #[cfg(test)] block
    struct MockApiClient {
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl ApiClient for MockApiClient {
        async fn upload_context(
            &self,
            _context: &ContextPayload,
        ) -> Result<(), CoreError> {
            if self.should_fail {
                Err(CoreError::Network("test failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn upload_success_saves_event() {
        let client = Arc::new(MockApiClient { should_fail: false });
        // ... test logic
    }

    #[tokio::test]
    async fn upload_failure_triggers_retry() {
        let client = Arc::new(MockApiClient { should_fail: true });
        // ... test logic
    }
}
```

### Ejecución de Pruebas

```bash
# Suite completa de pruebas
cargo test --workspace

# Crate específico
cargo test -p oneshim-core
cargo test -p oneshim-vision
cargo test -p oneshim-network

# Prueba individual
cargo test -p oneshim-storage -- sqlite::tests::migration_v7

# Pruebas de integración
cargo test -p oneshim-app
```

### Pruebas E2E (Panel Web)

```bash
cd crates/oneshim-web/frontend
pnpm test:e2e          # Suite completa de pruebas E2E
pnpm test:e2e:headed   # Con navegador visible
pnpm test:e2e:ui       # Modo UI de Playwright
```

## Proceso de PR

### Estrategia de Ramas

```bash
# Rama de nueva característica
git checkout -b feat/vision-pii-filter-improvement

# Rama de corrección de errores
git checkout -b fix/network-sse-reconnect

# Rama de documentación
git checkout -b docs/scheduler-architecture
```

### Lista de Verificación Pre-PR

Confirme todos los siguientes puntos antes de abrir un PR.

```bash
# 1. Verificación de formato
cargo fmt --check

# 2. Advertencias de clippy: 0
cargo clippy --workspace

# 3. Todas las pruebas pasan
cargo test --workspace

# 4. La compilación es exitosa
cargo build --workspace
```

### Redacción de la Descripción del PR

Incluya lo siguiente en la descripción de su PR:

- Motivación y contexto del cambio
- Resumen del enfoque de implementación
- Cómo probar el cambio
- Confirmación de que se respetan las reglas de arquitectura (especialmente las dependencias entre crates)

### Revisión de Código

Los revisores se enfocan en:

- Cumplimiento de Hexagonal Architecture (separación port/adapter)
- Sin dependencias directas entre crates adapter
- Advertencias de `cargo clippy`: 0
- Solo mocks manuales (sin mockall)
- Comentarios en inglés

## Convención de Mensajes de Commit

Siga [Conventional Commits](https://www.conventionalcommits.org/).

### Formato

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Tipos

| Tipo | Descripción |
|------|------|
| `feat` | Nueva característica |
| `fix` | Corrección de error |
| `perf` | Mejora de rendimiento |
| `refactor` | Refactorización (sin cambio de comportamiento) |
| `test` | Agregar o actualizar pruebas |
| `docs` | Cambios en la documentación |
| `chore` | Cambios de compilación, CI o dependencias |

### Ámbitos (Scopes)

Use el nombre del crate o el área funcional como ámbito.

`core`, `network`, `suggestion`, `storage`, `monitor`, `vision`, `tauri`, `web`, `automation`, `app`

### Ejemplos

```
feat(vision): add credit card number masking to PII filter

Masks 16-digit number patterns at Standard level and above.
Integrated with the existing CWE-359 compliance logic.
```

```
fix(network): cap SSE reconnect exponential backoff at 30 seconds

Prevents the retry delay from growing unbounded on repeated failures.
```

```
perf(storage): eliminate N+1 query in end_work_session with RETURNING

Merges the SELECT + UPDATE into a single RETURNING clause query.
Benchmark: 50% throughput improvement confirmed.
```

## Reporte de Problemas

### Reportes de Errores

Use la plantilla de **Bug Report** en GitHub Issues e incluya:

1. **Descripción del error**: Una explicación clara de lo que salió mal
2. **Pasos para reproducir**: Procedimiento paso a paso para la reproducción
3. **Comportamiento esperado**: Lo que debería ocurrir
4. **Comportamiento real**: Lo que realmente ocurre
5. **Entorno**: Sistema operativo, versión de Rust (`rustc --version`), versiones de dependencias relevantes
6. **Registros**: Salida relevante de `RUST_LOG=debug cargo run -p oneshim-app`

### Solicitudes de Características

Al proponer una característica, explíquela desde la perspectiva de Hexagonal Architecture:

- Si se necesita un nuevo port o si se puede extender un port existente
- En qué crate debería ubicarse el adapter
- Impacto en las relaciones de dependencia existentes entre crates

## Licencia

Al contribuir a este proyecto, usted acepta que sus contribuciones se licencian bajo la [Apache License 2.0](LICENSE).

---

Para preguntas, utilice GitHub Issues o Discussions.
