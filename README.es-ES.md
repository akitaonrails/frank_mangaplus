

<div align="center">
  <img src="docs/logo.png" width="320" alt="FRANK MANGA+" />

  <h1>FRANK MANGA+</h1>

  <p>
    <strong>Lector de escritorio para uso personal de <a href="https://mangaplus.shueisha.co.jp/">MANGA Plus by Shueisha</a>.</strong><br>
    Lee en Linux, macOS y Windows. El nivel gratuito funciona directamente; pega el secreto de suscriptor para acceder a los capítulos premium.
  </p>

  <p>
    <a href="https://github.com/akitaonrails/frank_mangaplus/releases/latest">Última versión</a>
    ·
    <a href="docs/install.md">Guía de instalación</a>
    ·
    <a href="docs/android-secret.md">Obtén tu secreto</a>
    ·
    <a href="docs/troubleshooting.md">Solución de problemas</a>
    ·
    <a href="docs/debugging.md">Contribuyentes</a>
  </p>

  <p>
    <a href="https://github.com/akitaonrails/frank_mangaplus/actions/workflows/ci.yml"><img src="https://github.com/akitaonrails/frank_mangaplus/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <a href="https://github.com/akitaonrails/frank_mangaplus/releases"><img src="https://img.shields.io/github/v/release/akitaonrails/frank_mangaplus?include_prereleases&label=release" alt="Release"></a>
    <img alt="Platforms" src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-blue">
    <img alt="License" src="https://img.shields.io/badge/license-MIT-green">
  </p>
</div>

---

## Aspecto

| Biblioteca / Búsqueda | Detalle del título | Lector |
|:---:|:---:|:---:|
| ![Biblioteca y búsqueda en todo el catálogo](docs/screenshots/library.png) | ![Detalle del título con lista de capítulos](docs/screenshots/title-detail.png) | ![Lector ajustado a página con desplazamiento por salto](docs/screenshots/reader.png) |
| Tus títulos marcados con favorito, más todo el catálogo si deseas explorar. | Arte del banner, sinopsis y la lista de capítulos (virtualizada: One Piece tiene más de 1100 y sigue desplazándose bien). | Modo página simple: una página ajustada a la ventana de visualización. La mitad izquierda avanza (lectura RTL de manga), la derecha retrocede. |

### Modo de doble página

Presiona `D` (o haz clic en el ícono de diseño en el encabezado) y el lector emparejará las páginas enfrentadas para que una doble página ocupe la pantalla tal como fue dibujada:

![Doble página en un monitor panorámico](docs/screenshots/reader-double-page.png)

Se alternan tres diseños al activar:
- **single** — una página a la vez
- **double** — pares secuenciales desde la página 1
- **double-cover** — la primera página de cada capítulo en solitario, luego pares (coincide con el manga impreso donde la portada se encuaderna sola antes de la primera doble página)

La elección se mantiene entre capítulos y sesiones.

### Filtro sepia para lectura nocturna

Presiona `F` (o haz clic en el ícono de luna creciente en el encabezado) para calentar los blancos de la página hacia el sepia. Los blancos brillantes de las pantallas LCD resultan agresivos en la oscuridad: esto los suaviza sin aplanar el contraste:

![Lector con el filtro sepia de protección ocular activo](docs/screenshots/reader-eye-protection.png)

Se alternan cuatro niveles al activar: **off → low → med → high → off**. El botón se tiñe de ámbar y muestra de uno a tres puntos para indicar el nivel activo. Se guarda en localStorage como las otras preferencias del lector.

Implementado como un filtro CSS `sepia + brightness + saturate` en toda la pila de páginas: el sepia desplaza el matiz hacia el ámbar mientras conserva el rango de luminosidad, por lo que los negros permanecen negros y el contraste del arte se mantiene intacto.

---

## Por qué existe

Pago por MANGA Plus y quería leerlo en un escritorio. Shueisha no proporciona un cliente de escritorio. Así que tienes dos opciones: instalar la aplicación de Android en una tableta (funciona, pero una tableta es una tableta), o mirar de cerca tu teléfono. Esta es la tercera opción.

Detrás de las escenas, utiliza la misma API que la aplicación oficial de Android. De forma predeterminada, la aplicación se registra como un dispositivo nuevo en el primer lanzamiento, utilizando el mismo flujo que la aplicación oficial al instalarla, por lo que obtienes acceso de nivel gratuito al catálogo directamente. Si ya eres un suscriptor de pago y deseas los capítulos de suscripción en el escritorio, puedes pegar el `deviceSecret` de tu teléfono en la aplicación para cambiar a tu sesión de suscriptor. No se evade ningún muro de pago.

## Lo que necesitas

Solo un binario descargado. Ejecútalo y la aplicación se registrará con la API oficial en su primera ejecución. Eso te da acceso de nivel gratuito a todo lo que MANGA Plus muestra de forma gratuita: todo el catálogo, los capítulos más recientes y los primeros de cada serie, y el backlog de rotación gratuita.

Si también deseas los capítulos bloqueados por suscripción en el escritorio:

- Una suscripción activa de MANGA Plus en una instalación en el teléfono.
- Tu `deviceSecret` de esa instalación. 5 minutos si tu teléfono tiene root (`adb shell`), unos 20 si no lo tiene (emulador de Android con root en tu escritorio: guía paso a paso en [docs/android-secret.md](docs/android-secret.md)).
- Pégalo en la configuración de la aplicación. Reemplaza la sesión de nivel gratuito con la de tu suscriptor.

## Instalación

Descarga la compilación para tu sistema operativo desde la [página de versiones](https://github.com/akitaonrails/frank_mangaplus/releases/latest):

| Sistema operativo | Archivo |
|---|---|
| Linux (AppImage) | `FRANK.MANGA+_*_amd64.AppImage` |
| Linux (.deb) | `FRANK.MANGA+_*_amd64.deb` |
| Linux (Arch) | `yay -S mangaplus-reader-bin` |
| macOS (Apple Silicon) | `FRANK.MANGA+_*_aarch64.dmg` |
| macOS (Intel) | `FRANK.MANGA+_*_x64.dmg` |
| Windows | `FRANK.MANGA+_*_x64-setup.exe` |

Documentación detallada de instalación: [docs/install.md](docs/install.md). ¿Pantalla en blanco en Linux? Consulta [docs/troubleshooting.md](docs/troubleshooting.md): el binario se recupera automáticamente de los fallos de EGL en el segundo lanzamiento y hay anulaciones del modo de renderizado para casos complicados.

En el primer lanzamiento, la aplicación llama al punto final oficial `/register` y se le asigna un `deviceSecret` de nivel gratuito nuevo. Esto se guarda localmente y ya puedes leer el catálogo. Si eres un suscriptor de pago y también deseas los capítulos de suscripción, abre Configuración y pega tu `deviceSecret` extraído del teléfono para actualizar.

## Qué incluye

Una vista de biblioteca de tus títulos marcados con favorito. La página de búsqueda consulta todo el catálogo en inglés y filtra localmente mientras escribes.

El detalle del título muestra el arte del banner, la sinopsis y la lista completa de capítulos. La lista está virtualizada, por lo que una serie con mil capítulos se desplaza correctamente. Hay un botón para cambiar el orden y un botón "Continuar ▶" que salta al último capítulo que abriste.

El lector está ajustado a la página y tiene desplazamiento por salto. Haz clic en la mitad izquierda de una página para avanzar y en la derecha para retroceder (dirección de lectura RTL de manga). También funcionan las teclas de flecha, Espacio, j/k y AvPág/RePág. Cuando llegas al final de un capítulo, el siguiente se pre-descarga y se agrega al desplazamiento, así no tienes que volver a la lista de capítulos cada vez. La reanudación por capítulo es automática: sal en medio de la lectura y la próxima vez que abras ese capítulo, llegarás a la página donde te quedaste.

Cada página que cargues se almacena en caché en `~/.cache/mangaplus-reader/`. Reabrir el mismo capítulo es instantáneo después de la primera lectura.

El estado de lectura se guarda en localStorage. La lista de capítulos marca el capítulo donde te quedaste con una insignia de "Última vez", y la página del título muestra un enlace para volver a él.

## Cómo funciona

```
┌─────────────────────────────┐      ┌──────────────────────────┐
│  Tauri WebView (SvelteKit)  │      │  Rust client (reqwest)   │
│                             │      │                          │
│  Library / Search / Reader  │◄────►│  get_chapter_pages…      │
│  <img src="mpimg://…">      │      │  get_title_detail…       │
└──────────┬──────────────────┘      │  fetch_image (cookies,   │
           │                         │       okhttp UA, cache)  │
           │ mpimg:// scheme         └────────────┬─────────────┘
           │ intercepted by Tauri                 │
           └─────────────────────────────────────►│ HTTPS
                                                  ▼
                                       jumpg-api.tokyo-cdn.com
                                       jumpg-assets3.tokyo-cdn.com
```

`mangaplus-api` es un paquete puro de Rust con pruebas basadas en fixtures. Realiza la decodificación de protobuf, el manejo de cookies y el protocolo `plus_vw_token` que espera la CDN de imágenes premium. Sin dependencias de Tauri.

`mangaplus-desktop` es la aplicación Tauri 2 + SvelteKit. Las URL de las imágenes pasan por un esquema de URI personalizado `mpimg://` que hace de proxy de la solicitud de vuelta a través del mismo cliente de Rust. Así es como las cookies sobreviven al límite WebView/Rust.

Cada solicitud va directamente a la CDN de Shueisha, sin proxy intermedio.

Si no confías en un binario aleatorio de internet para hacer eso de buena fe (y no deberías), el formato real que viaja por la red: cada encabezado, cada detalle al que me enfrenté al hacer ingeniería inversa, está documentado en [docs/debugging.md](docs/debugging.md). Puedes verificarlo tú mismo.

## Documentación

- [`docs/install.md`](docs/install.md): instalación para usuarios finales. Registro automático de nivel gratuito y actualización opcional a suscriptor.
- [`docs/android-secret.md`](docs/android-secret.md): guía paso a paso para extraer un `deviceSecret` de suscriptor usando un AVD con root. No interactúa con tu teléfono físico.
- [`docs/troubleshooting.md`](docs/troubleshooting.md): pantalla en blanco en Linux, anulaciones del modo de renderizado, soluciones alternativas para fallos de WebKitGTK y cómo la recuperación automática se encarga de todo eso por ti.
- [`docs/debugging.md`](docs/debugging.md): notas para contribuyentes. mitmproxy, Frida, los encabezados reales y los problemas con los que me topé.

## Aviso legal

No afiliado con Shueisha ni Manga Plus. El flujo de registro nuevo predeterminado reproduce el mismo protocolo que realiza la aplicación oficial de Android en una instalación nueva: solo otorga acceso de nivel gratuito. El contenido bloqueado por suscripción requiere pegar un `deviceSecret` extraído de tu propia instalación de pago en el teléfono. No se evade ningún muro de pago, suscripción ni DRM.

## Licencia

MIT.
