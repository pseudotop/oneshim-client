# Política de Seguridad

Nos tomamos muy en serio la seguridad del Cliente Rust de ONESHIM. Si descubre una vulnerabilidad, por favor siga los procedimientos descritos en este documento para reportarla.

## Reporte de una Vulnerabilidad de Seguridad

**No reporte vulnerabilidades de seguridad como issues públicos.** Por favor, utilice los canales privados que se indican a continuación.

### Cómo Reportar

1. **Correo electrónico**: Envíe un correo a `security@oneshim.dev`. Por favor, utilice cifrado PGP si es posible.
2. **GitHub Security Advisory**: Puede reportar de forma privada seleccionando "Report a vulnerability" en la pestaña "Security" del repositorio.

### Información a Incluir en su Reporte

Para permitir una respuesta efectiva, por favor incluya la mayor cantidad posible de la siguiente información.

- **Tipo de Vulnerabilidad**: Identificador CWE si corresponde (por ejemplo, CWE-79 XSS, CWE-89 Inyección SQL, CWE-200 Exposición de Información)
- **Crate Afectado**: El nombre del crate que contiene la vulnerabilidad y la ruta del archivo fuente (por ejemplo, `crates/oneshim-vision/src/privacy.rs`)
- **Pasos para Reproducir**: Instrucciones paso a paso para reproducir la vulnerabilidad
- **Impacto**: El impacto esperado si la vulnerabilidad es explotada (exposición de datos locales, ejecución remota de código, etc.)
- **Prueba de Concepto (PoC)**: Código o capturas de pantalla que demuestren la vulnerabilidad, si están disponibles
- **Corrección Sugerida**: Cualquier idea para corregir la vulnerabilidad, si la tiene (opcional)
- **Entorno**: Sistema operativo, versión de Rust (`rustc --version`), versión de cargo y versiones de crates relevantes

### Áreas de Seguridad de Especial Importancia

Las siguientes áreas son de particular importancia en materia de seguridad en el Cliente Rust de ONESHIM.

- **Captura de Pantalla y Filtro de PII** (`oneshim-vision`): Elusión del enmascaramiento de información de identificación personal en pantalla
- **Almacenamiento Local SQLite** (`oneshim-storage`): Acceso no autorizado a datos sin cifrar
- **Tokens de Autenticación JWT** (`oneshim-network`): Robo de tokens o elusión de la validación
- **Control de Automatización** (`oneshim-automation`): Ejecución arbitraria de comandos mediante elusión de la validación de políticas
- **Actualización Automática** (`oneshim-app`): Elusión de la verificación de integridad de los binarios de actualización
- **Panel Web Local** (`oneshim-web`): Acceso no autorizado a la API local (relacionado con la configuración `allow_external`)

## Versiones Soportadas

Las actualizaciones de seguridad se proporcionan para las siguientes versiones.

| Versión | Estado de Soporte |
|---------|---------------|
| Última rama `main` | Soportada |
| Última etiqueta de lanzamiento | Soportada |
| Lanzamientos anteriores | No soportados |

Dado que aún no se ha realizado un lanzamiento oficial, por favor reporte las vulnerabilidades de seguridad contra la **última rama `main`**.

## SLA de Tiempo de Respuesta

Al recibir un reporte de seguridad, nos comprometemos a responder de acuerdo con la siguiente cronología.

| Fase | Plazo Objetivo |
|-------|----------------|
| Acuse de recibo | Dentro de 3 días hábiles |
| Evaluación de la vulnerabilidad y plan de respuesta | Dentro de 14 días |
| Lanzamiento del parche | Dentro de 90 días |
| Notificación al reportante y cronograma de divulgación | Inmediatamente después del lanzamiento del parche |

Para problemas de seguridad urgentes (vulnerabilidades de alta gravedad como ejecución remota de código o elusión completa de autenticación), por favor incluya `[URGENT]` en el asunto del correo electrónico para recibir atención prioritaria.

## Política de Divulgación Responsable

El Cliente Rust de ONESHIM sigue una política de **Divulgación Responsable**.

### Nuestros Compromisos

- Protegeremos la privacidad del reportante.
- Notificaremos al reportante una vez completada la corrección y coordinaremos el cronograma de divulgación con su consentimiento.
- Acreditaremos al reportante en el Security Advisory por su contribución (si lo desea).
- No emprenderemos acciones legales contra actividades de investigación de seguridad realizadas de buena fe.

### Nuestras Solicitudes a los Reportantes

- Por favor, absténgase de divulgar públicamente hasta que la vulnerabilidad haya sido corregida.
- Por favor, asegúrese de que su validación de la vulnerabilidad no impacte los datos o servicios de otros usuarios.
- No destruya, modifique ni extraiga datos sin autorización previa.

## Contacto de Seguridad

| Canal | Contacto |
|---------|---------|
| Correo de Seguridad | `security@oneshim.dev` |
| GitHub Security Advisory | Pestaña Security del repositorio |

## Notificaciones de Actualizaciones de Seguridad

Las actualizaciones de seguridad se anunciarán a través de los siguientes canales.

- GitHub Security Advisories
- Notas de lanzamiento (CHANGELOG.md)
- Página de GitHub Releases

## Referencias de Integridad

- Línea base de integridad autónoma: `docs/security/standalone-integrity-baseline.md`
- Runbook de integridad: `docs/security/integrity-runbook.md`
- Script de verificación de integridad local: `scripts/verify-integrity.sh`

---

Agradecemos a todas las personas que contribuyen a mejorar nuestra seguridad.
