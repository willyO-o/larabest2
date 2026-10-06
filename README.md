# LARABEST

Servidor local para Windows 10 y 11 y para macOS 13 o posterior, inspirado en Laragon: gestiona **Nginx**, **PHP** y **MariaDB** como procesos propios y crea un dominio `<carpeta>.test` por cada proyecto de `www/`. Hecho con Tauri 2, Rust y Vue 3.

La interfaz sigue la estructura de Laragon (ventana compacta, una fila por servicio, cinco botones abajo y un menú contextual en árbol) con identidad propia. La banda superior cambia de color con el estado: grafito detenido, ámbar iniciando, verde todo en marcha, rojo con un fallo.

**Fase 1:** iniciar y detener todos los servicios o uno a la vez, cambiar la versión de PHP, crear un sitio `.test` por carpeta, vivir en la bandeja del sistema, y los botones Web, Base de datos, Terminal y Raíz.

**Fase 2:** crear un sitio en blanco, WordPress o Laravel con un clic, HTTPS opcional con una autoridad certificadora local, acceso a configuraciones y logs desde el menú, y tema Sistema/Claro/Oscuro.

**Fase 3:** la misma app en macOS, con Nginx, PHP (`php-fpm`) y MariaDB instalados con Homebrew. Ver [macOS](#macos).

## Menú contextual

Clic derecho en cualquier parte de la ventana, el botón **Menú** o el ícono de la bandeja abren el mismo menú:

| Menú | Contenido |
| --- | --- |
| Iniciar todo / Detener todo | según el estado |
| www | un sitio por carpeta (abre `http://<carpeta>.test`), **Crear sitio** (en blanco, WordPress, Laravel), abrir la carpeta www, actualizar sitios y archivo hosts |
| Nginx | versión, recargar configuración, carpeta de configuración propia por sitio (`etc\nginx\custom\<sitio>.conf`), `nginx.conf` generado, logs |
| PHP | versión, `php.ini` de ajustes propios, extensiones (marcar o desmarcar), `php.ini` generado, log de errores |
| MariaDB | versión, `my.cnf` de ajustes propios, Adminer, `my.cnf` generado, carpeta de datos, log de errores |
| Logs | errores y accesos de Nginx, errores de PHP y de MariaDB, carpeta de logs |
| Herramientas | Web, Base de datos, Terminal, carpeta raíz |
| Preferencias | tema (Sistema, Claro, Oscuro), iniciar todo al abrir, HTTPS para los sitios `.test`, carpeta de certificados, cambiar la carpeta raíz, abrir `app.json` |

Lo generado (`etc\nginx\nginx.conf`, `etc\php\php.ini`, `etc\mariadb\my.cnf`) se regenera en cada inicio: ábrelo solo para consultarlo. Tus ajustes van en archivos que nunca se regeneran:

- PHP: `etc\php\custom\user.ini` (PHP lo carga con `PHP_INI_SCAN_DIR` y tiene prioridad sobre el generado).
- MariaDB: `etc\mariadb\custom\user.cnf` (incluido con `!includedir`).
- Nginx: `etc\nginx\custom\<sitio>.conf` (incluido dentro del `server` de ese sitio).

Las extensiones marcadas en el menú se guardan en `app.json` y PHP se reinicia solo si estaba en marcha. Al cambiar de versión, se reinicia solo ese servicio.

## Crear un sitio

Botón **Nuevo sitio** en la línea de estado, o **Menú → www → Crear sitio**. Escribe el nombre de la carpeta (letras, números y guiones) y elige la plantilla:

| Plantilla | Qué hace |
| --- | --- |
| En blanco | Crea `www\<nombre>\index.php` con una página de bienvenida. |
| WordPress | Crea la base de datos `<nombre>` (con `-` cambiado por `_`), descarga WordPress en español desde `es.wordpress.org` la primera vez (queda en `tmp\downloads\` para los siguientes sitios), lo extrae y escribe `wp-config.php` con claves únicas. Al abrir el sitio aparece el instalador de WordPress en español. |
| Laravel | Crea la base de datos, descarga `composer.phar` en `bin\composer\` si falta, ejecuta `composer create-project laravel/laravel <nombre>` con el PHP activo, configura `.env` para MariaDB (root sin contraseña) y ejecuta `php artisan migrate`. |

MariaDB se inicia sola si hace falta. Si algo falla antes de terminar, la carpeta creada a medias se borra para poder reintentar; si solo fallan las migraciones de Laravel, el proyecto se conserva y se avisa. WordPress y Laravel necesitan internet (la primera descarga de WordPress y cada `composer create-project`).

## HTTPS

**Menú → Preferencias → HTTPS para los sitios .test**. La primera vez:

1. La app crea una autoridad certificadora local (ECDSA P-256). Su clave privada se guarda en la carpeta de datos de tu usuario (`%LOCALAPPDATA%\com.soglotec.larabest\ca\`), no en `C:\LARABEST`, para que otros usuarios del equipo no puedan leerla.
2. Windows muestra un aviso de seguridad para instalar el certificado raíz «LARABEST Local CA» solo para tu usuario (no hace falta ser administrador). Acéptalo; si lo cancelas, HTTPS no se activa.
3. Cada sitio recibe su certificado en `etc\ssl\<sitio>.test.crt` y Nginx escucha también en el puerto 443. Los sitios siguen respondiendo por http.

Chrome y Edge usan el almacén de Windows. Firefox lo usa si `security.enterprise_roots.enabled` está activo (lo está por defecto en Windows en versiones recientes). Desactivar HTTPS no desinstala el certificado raíz: se quita desde `certmgr.msc` → Entidades de certificación raíz de confianza.

## Carpeta de datos

Por defecto es `C:\LARABEST`. Se puede cambiar en **Menú → Preferencias** (el cambio se aplica al reiniciar). La ruta no puede tener espacios porque Nginx y PHP fallan con ellas.

```text
C:\LARABEST\
  bin\            binarios copiados a mano (ver abajo)
  www\            una subcarpeta = un sitio <carpeta>.test
  etc\            configuración generada (no editar; se regenera)
    nginx\custom\ tus inclusiones por sitio: <sitio>.conf (nunca se regeneran)
    php\custom\   user.ini con tus ajustes de PHP (nunca se regenera)
    mariadb\custom\ user.cnf con tus ajustes de MariaDB (nunca se regenera)
  data\mariadb\   bases de datos
  logs\           logs de Nginx, PHP y MariaDB
  tmp\            PID y temporales
  app.json        versión activa de cada servicio, puertos
```

La app crea todas estas carpetas la primera vez que se abre.

## Preparar la carpeta `bin\`

En la fase 1 los binarios se descargan y copian a mano. Cada versión va en su propia carpeta, con el número de versión como nombre. La app la detecta si encuentra el ejecutable indicado.

| Servicio | De dónde | Qué descargar | Carpeta destino | La app busca |
| --- | --- | --- | --- | --- |
| Nginx | <https://nginx.org/en/download.html> | *Stable version* → `nginx/Windows-1.28.x` (zip) | `bin\nginx\1.28.0\` | `nginx.exe` |
| PHP 8.3 | <https://windows.php.net/download/> | PHP 8.3 → **VS16 x64 Non Thread Safe** (zip) | `bin\php\8.3.x\` | `php-cgi.exe` |
| PHP 8.4 | <https://windows.php.net/download/> | PHP 8.4 → **VS17 x64 Non Thread Safe** (zip) | `bin\php\8.4.x\` | `php-cgi.exe` |
| MariaDB 11.4 | <https://mariadb.org/download/> | MariaDB Server 11.4 (LTS), *MS Windows (64-bit)*, tipo **ZIP file** | `bin\mariadb\11.4.x\` | `bin\mariadbd.exe` |
| Adminer | <https://www.adminer.org/#download> | *Adminer for MySQL* (`adminer-x.y.z-mysql.php`) | `bin\adminer\` renombrado a **`adminer.php`** | `adminer.php` |
| Composer (opcional: se descarga solo al crear un sitio Laravel) | <https://getcomposer.org/download/> | `composer.phar` | `bin\composer\composer.phar` | `composer.phar` |

Pasos:

1. Descomprime cada zip y renombra la carpeta resultante con el número de versión. Por ejemplo, `php-8.4.6-nts-Win32-vs17-x64` pasa a ser `bin\php\8.4.6\`, y `mariadb-11.4.5-winx64` pasa a ser `bin\mariadb\11.4.5\`.
2. Comprueba que el ejecutable quede directamente dentro de la carpeta de versión (`bin\php\8.4.6\php-cgi.exe`) y no en una subcarpeta extra.
3. PHP usa builds **NTS x64** (Non Thread Safe). Las *Thread Safe* no sirven con `php-cgi`.
4. PHP y MariaDB necesitan el **runtime de Visual C++ 2015-2022 x64**: <https://aka.ms/vs/17/release/vc_redist.x64.exe>. Si falta, la fila del servicio lo dice.
5. No hace falta tocar `php.ini`, `nginx.conf` ni `my.cnf`: la app los genera en `etc\`.

Resultado esperado:

```text
bin\
  nginx\1.28.0\nginx.exe
  php\8.3.20\php-cgi.exe
  php\8.4.6\php-cgi.exe
  mariadb\11.4.5\bin\mariadbd.exe
  adminer\adminer.php
  composer\composer.phar      (opcional)
```

Con varias versiones de PHP se usa la más reciente hasta que elijas otra en **Menú → PHP → Versión** o en la bandeja.

## Instalar el `.exe` desde GitHub Actions

1. En GitHub ve a **Actions → build-windows** y abre la última ejecución en verde de la rama o del pull request.
2. Al final de la página, en **Artifacts**, descarga `instalador-windows` (un `.zip`).
3. Descomprímelo y ejecuta `LARABEST_0.1.0_x64-setup.exe`.
4. El instalador no está firmado, así que SmartScreen avisará: pulsa **Más información → Ejecutar de todas formas**. Se instala solo para tu usuario y no pide administrador.
5. Si la PC no tiene WebView2 (en Windows 10 es raro), el instalador lo descarga. Sin internet, compila con `webviewInstallMode: offlineInstaller`.

Para desinstalar: **Configuración → Aplicaciones → LARABEST → Desinstalar**. La carpeta `C:\LARABEST` (proyectos y bases de datos) no se borra.

## Pruebas manuales en Windows 10

Prepara `bin\` como se describe arriba y crea `www\blog\index.php` con `<?php phpinfo();` y `www\tienda\public\index.php` con `<?php echo "Laravel";`.

**Arranque y bandeja**

- [ ] Al abrir aparece la ventana compacta con Nginx, PHP y MariaDB en *Detenido* y con su versión y puerto.
- [ ] Aparece un aviso de UAC para el archivo hosts (los sitios `blog.test`, `tienda.test` y `adminer.test`). Al aceptarlo, `C:\Windows\System32\drivers\etc\hosts` tiene un bloque `# >>> LARABEST` y el resto del archivo no cambió.
- [ ] Cerrar la ventana con la X la oculta; el ícono sigue en la bandeja. Un clic en el ícono la vuelve a mostrar.
- [ ] Abrir la app por segunda vez no abre otra instancia: muestra la ventana existente.

**Servicios**

- [ ] **Iniciar todo**: los tres pasan por *Iniciando…* a *En marcha* en menos de 15 s. No aparece ninguna ventana de consola.
- [ ] La primera vez MariaDB crea `data\mariadb\` (con `mariadb-install-db`).
- [ ] En el Administrador de tareas hay 4 procesos `php-cgi.exe`, `nginx.exe` (2) y `mariadbd.exe`.
- [ ] **Detener** detiene los tres y no quedan esos procesos.
- [ ] Iniciar y detener cada servicio por separado con el botón de su fila.
- [ ] El botón de recargar de Nginx no reinicia el proceso (el PID sigue igual).

**Sitios**

- [ ] `http://blog.test` muestra `phpinfo()`, con las extensiones curl, fileinfo, gd, intl, mbstring, openssl, pdo_mysql, mysqli, zip y exif cargadas.
- [ ] `http://tienda.test` muestra "Laravel" (sirve desde `public\`).
- [ ] Con los servicios en marcha, crea `www\nuevo\index.html`: aparece en **Menú → Sitios**, se pide UAC una sola vez y `http://nuevo.test` funciona sin reiniciar nada.
- [ ] Borra `www\nuevo`: desaparece del menú y su línea del archivo hosts.
- [ ] Si cancelas el UAC, el sitio muestra un aviso ⚠ y **Reintentar** vuelve a pedir permiso.
- [ ] Una carpeta con espacios (`www\mi sitio`) se ignora.

**PHP**

- [ ] **Menú → PHP → Versión → 8.3.x** con los servicios en marcha: solo PHP se reinicia (Nginx y MariaDB siguen *En marcha*) y `phpinfo()` muestra 8.3.
- [ ] Al cerrar y abrir la app se mantiene la versión elegida (`app.json`).
- [ ] La bandeja marca la versión activa en su submenú PHP.

**Botones**

- [ ] **Web** abre `http://localhost/` con la lista de carpetas de `www\`.
- [ ] **Base de datos** abre Adminer en `http://adminer.test`. Entra con servidor `127.0.0.1`, usuario `root` y sin contraseña. Crea una base de datos.
- [ ] **Terminal** abre una consola en `www\` donde `php -v`, `mariadb --version`, `nginx -v` y `composer -V` (si copiaste Composer) funcionan. En una consola normal de Windows esos comandos siguen sin existir.
- [ ] **Raíz** abre `C:\LARABEST` en el Explorador.

**Errores visibles**

- [ ] Con otro programa usando el puerto 80 (por ejemplo IIS, XAMPP o `python -m http.server 80`), **Iniciar todo** deja Nginx en *Error* con "El puerto 80 está ocupado por …" en su fila, y PHP y MariaDB arrancan igual.
- [ ] Sin el runtime de Visual C++, PHP queda en *Error* con un mensaje que lo nombra.
- [ ] Si matas `mariadbd.exe` desde el Administrador de tareas, MariaDB pasa a *Error* ("terminó inesperadamente") y no se reinicia solo.

**Menú contextual y configuraciones**

- [ ] Clic derecho en la ventana, el botón **Menú** y el ícono de la bandeja abren el mismo menú en árbol.
- [ ] **PHP → Extensiones**: marca `sodium`; PHP se reinicia solo si estaba en marcha y `phpinfo()` muestra sodium. Desmárcala y desaparece.
- [ ] **PHP → php.ini (ajustes propios)** abre `etc\php\custom\user.ini`. Escribe `memory_limit = 1G`, pulsa **Recargar** en la fila de PHP y `phpinfo()` muestra 1G.
- [ ] **MariaDB → my.cnf (ajustes propios)** abre `etc\mariadb\custom\user.cnf`; tras reiniciar MariaDB se aplican los cambios.
- [ ] **Logs → PHP: errores** abre el log aunque esté vacío.
- [ ] **Preferencias → Iniciar todo al abrir**: al cerrar y abrir la app, los servicios arrancan solos.
- [ ] Con los servicios detenidos, la banda es gris; al iniciar pasa a ámbar y luego a verde; con un fallo, a rojo con el nombre del servicio.

**Fase 2: crear sitios**

- [ ] **Nuevo sitio → En blanco** con el nombre `demo`: aparece `www\demo\index.php`, el paso a paso termina en «Sitio listo» y **Abrir sitio** muestra la página de bienvenida.
- [ ] Un nombre con espacios (`mi sitio`) o repetido se marca en rojo y **Crear sitio** queda deshabilitado.
- [ ] **WordPress** con el nombre `blog-wp` y MariaDB detenido: MariaDB arranca solo, se crea la base `blog_wp` (verla en Adminer), se descarga WordPress y `http://blog-wp.test` abre el instalador en español sin pedir datos de la base.
- [ ] Un segundo sitio WordPress no vuelve a descargar (paso «Usando la copia descargada de WordPress»).
- [ ] **Laravel** con el nombre `tienda`: se descarga Composer si faltaba, `composer create-project` termina, `www\tienda\.env` tiene `DB_CONNECTION=mariadb` y `DB_DATABASE=tienda`, la base tiene las tablas de Laravel y `http://tienda.test` muestra la página de Laravel.
- [ ] Sin internet, WordPress falla con un mensaje claro y no queda la carpeta a medias.
- [ ] En la **Terminal**, `php -m` lista las mismas extensiones que `phpinfo()` (curl, openssl, pdo_mysql…) y `composer -V` funciona.

**Fase 2: HTTPS**

- [ ] **Preferencias → HTTPS**: Windows pide confirmar el certificado «LARABEST Local CA»; al aceptar, la línea de estado muestra «HTTPS» y `https://demo.test` abre en Chrome/Edge con candado y sin advertencias.
- [ ] Si se cancela el aviso, aparece el error «HTTPS sigue desactivado…» y los sitios siguen en http.
- [ ] Con HTTPS activo, un sitio nuevo abre directamente en `https://` y Adminer en `https://adminer.test`.
- [ ] Con el puerto 443 ocupado (por ejemplo por IIS o Skype), Nginx queda en *Error* con «El puerto 443 está ocupado por …».
- [ ] Al desactivar HTTPS, los vhosts dejan de escuchar en 443 y los sitios vuelven a http.

**Tema**

- [ ] Los botones monitor/sol/luna de la banda cambian entre Sistema, Claro y Oscuro, también la barra de título de Windows, y se mantienen al reiniciar la app.
- [ ] Con «Sistema», al cambiar el tema de Windows (Configuración → Personalización → Colores) la app cambia sola.
- [ ] Al abrir la app no hay un destello blanco antes de pintar en tema oscuro.

**Salida**

- [ ] **Salir** desde la bandeja detiene los servicios (MariaDB de forma ordenada) y cierra la app.
- [ ] Si la app se cierra a la fuerza (Administrador de tareas → Finalizar tarea), Nginx, PHP y MariaDB también se cierran.

## macOS

En macOS la app usa los programas de **Homebrew** en vez de copiarlos a `bin/`. Funciona en Apple Silicon y en Intel (el `.dmg` es universal) desde macOS 13 Ventura; Homebrew ofrece binarios listos para macOS 14 o posterior (en versiones anteriores, y en Intel para algunas fórmulas, los compila y tarda bastante).

### Instalar los servicios

1. Instala Homebrew si no lo tienes: <https://brew.sh> (pide la contraseña de administrador una vez).
2. En la Terminal:

   ```bash
   brew install nginx php@8.3 php@8.4 mariadb@11.4
   ```

   No hace falta `brew services start`: la app arranca y detiene cada programa con su propia configuración. Si ya tenías alguno corriendo con `brew services`, detenlo (`brew services stop nginx`, etc.) para liberar los puertos.
3. Opcional, igual que en Windows: `adminer.php` en `~/LARABEST/bin/adminer/` y `composer.phar` en `~/LARABEST/bin/composer/` (Composer se descarga solo al crear un sitio Laravel).

La app encuentra las versiones en `/opt/homebrew/Cellar` (Apple Silicon) o `/usr/local/Cellar` (Intel). Cambiar de versión de PHP funciona igual que en Windows (**Menú → PHP → Versión**). Si `brew upgrade` cambia 8.4.13 por 8.4.14, la app sigue en la rama 8.4.

### Instalar la app (sin firma de Apple)

La app no está firmada con un certificado de Apple Developer (cuesta 99 USD al año). No hace falta para usarla: macOS la instala igual, pero la primera vez bloquea la apertura y hay que permitirla una sola vez.

1. En GitHub ve a **Actions → build-macos**, abre la última ejecución en verde y descarga el artefacto `instalador-macos`.
2. Descomprime el `.zip`, abre `LARABEST_0.1.0_universal.dmg` y arrastra LARABEST a **Aplicaciones**.
3. Ábrela. macOS dice que no puede verificar al desarrollador: pulsa **Listo** (o **Cancelar**).
4. Ve a **Configuración del Sistema → Privacidad y seguridad**, baja hasta «Se bloqueó el uso de LARABEST» y pulsa **Abrir igualmente**; confirma con tu contraseña. En macOS 14 o anterior también sirve: clic derecho sobre la app → **Abrir** → **Abrir**.
5. Desde ese momento se abre normalmente.

Alternativa por Terminal (quita la marca de «descargado de internet»): `xattr -dr com.apple.quarantine /Applications/LARABEST.app`.

Si macOS dice que la app «está dañada», es la misma marca de cuarentena: usa el comando anterior.

### Diferencias con Windows

| | Windows | macOS |
| --- | --- | --- |
| Programas | zip copiados en `C:\LARABEST\bin\` | Homebrew (`brew install`) |
| PHP | 4 procesos `php-cgi` (puertos 9000–9003) | un `php-fpm` en el puerto 9000 que reparte entre sus procesos |
| Extensiones de PHP | se marcan en **Menú → PHP → Extensiones** | vienen compiladas en el PHP de Homebrew (pdo_mysql, intl, gd, zip, sodium…); para otras, `pecl install` y `extension=` en `user.ini` |
| Carpeta raíz | `C:\LARABEST` | `~/LARABEST` |
| Archivo hosts | aviso de UAC | `/etc/hosts`, con la contraseña de administrador (aviso estándar de macOS) |
| HTTPS | almacén de certificados del usuario (`certutil`) | llavero de inicio de sesión, «Confiar siempre» (pide tu contraseña) |
| Terminal | `cmd.exe` | Terminal.app con tu shell (zsh o bash); PHP, MariaDB, Nginx y Composer de la app quedan primero en el PATH aunque tu `.zshrc` agregue Homebrew |
| MariaDB | puerto 3306 | puerto 3306 y socket propio `~/LARABEST/tmp/mariadb.sock` (en la Terminal de la app `mariadb -u root` lo usa solo) |
| Bandeja | ícono en la bandeja; clic izquierdo abre la ventana | ícono en la barra de menús; el clic abre el menú. Clic en el ícono del Dock muestra la ventana |

Nginx escucha en todas las interfaces: en macOS los puertos 80 y 443 solo se pueden abrir sin ser administrador así. Si el firewall de macOS está activo, puede preguntar si `nginx` acepta conexiones entrantes; para trabajar en local da igual la respuesta. Si el puerto 80 está ocupado y la fila no dice por quién, suele ser el Apache que trae macOS: `sudo apachectl stop`.

Para quitar la CA de HTTPS: **Acceso a Llaveros → inicio de sesión → Certificados → LARABEST Local CA → Eliminar**. Firefox usa su propio almacén: impórtala en sus ajustes de certificados si lo usas.

### Pruebas manuales en macOS

Instala los servicios con Homebrew y la app como se indica arriba. Crea `~/LARABEST/www/blog/index.php` con `<?php phpinfo();`.

- [ ] Al abrir aparece la ventana con Nginx, PHP y MariaDB *Detenido*, con las versiones de Homebrew. Sin Homebrew, cada fila dice qué `brew install` falta.
- [ ] macOS pide la contraseña de administrador para `/etc/hosts`; al aceptarla, `/etc/hosts` tiene el bloque `# >>> LARABEST` y lo demás no cambió. Si se cancela, los sitios muestran el aviso ⚠ y **Reintentar** vuelve a pedirla.
- [ ] **Iniciar todo**: los tres llegan a *En marcha*. En Monitor de Actividad hay `nginx` (2), `php-fpm` (varios) y `mariadbd`.
- [ ] `http://blog.test` muestra `phpinfo()` de 8.4 con pdo_mysql, intl, gd, mbstring y zip.
- [ ] **Menú → PHP → Versión → 8.3.x**: solo PHP se reinicia y `phpinfo()` muestra 8.3.
- [ ] **Base de datos** abre Adminer; entra con `root` sin contraseña.
- [ ] **Nuevo sitio → WordPress** y **→ Laravel** funcionan igual que en Windows.
- [ ] **Terminal** abre Terminal.app en `www/` con `php -v` (versión activa), `composer -V` y `mariadb -u root -e 'select 1'` funcionando; en una Terminal normal, `php` es el de tu sistema o no existe.
- [ ] **Preferencias → HTTPS**: macOS pide tu contraseña para confiar en «LARABEST Local CA»; `https://blog.test` abre en Safari y Chrome con candado.
- [ ] Cerrar la ventana la oculta; el clic en el ícono del Dock o **Mostrar ventana** en la barra de menús la vuelve a mostrar.
- [ ] El ícono de la barra de menús se ve en tema claro y oscuro.
- [ ] **Menú → PHP → php.ini (ajustes propios)** abre `user.ini` en el editor de texto.
- [ ] **Salir** (o Cmd+Q) detiene los servicios antes de cerrar. Si la app se cierra a la fuerza, al abrirla de nuevo detiene los procesos que quedaron (no aparece «puerto ocupado»).

El CI de macOS ya prueba, con los programas reales de Homebrew: arranque de los tres servicios, un sitio servido por Nginx y php-fpm, la base de datos con root sin contraseña, Adminer, la creación de un sitio WordPress y uno Laravel, la Terminal, HTTPS, el cambio de versión de PHP y la limpieza de procesos huérfanos. Lo que no prueba (necesita una persona frente al Mac): los avisos de contraseña, la ventana, la barra de menús y abrir la app descargada.

## Desarrollo

Requisitos: Node 22, Rust estable y, en Windows, las herramientas de compilación de Visual Studio con WebView2.

```bash
npm ci
npm run tauri dev        # app en modo desarrollo (Windows)
npm run build            # vue-tsc + vite
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

En Linux se compila y se prueba el núcleo (dominio, drivers y casos de uso) con adaptadores falsos, y también los adaptadores de macOS que solo usan Unix (procesos, terminal, puertos). La app no gestiona servicios en Linux. Para revisar el código de otras plataformas desde Linux:

```bash
rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin x86_64-apple-darwin
cargo clippy --target x86_64-pc-windows-msvc --all-targets
# objc2 compila un archivo Objective-C sin cabeceras: basta con clang
CC_aarch64_apple_darwin=clang AR_aarch64_apple_darwin=llvm-ar \
  cargo clippy --target aarch64-apple-darwin --all-targets
```

En un Mac con Homebrew, la prueba de punta a punta (`cargo test --lib -- --ignored --nocapture e2e_homebrew`) arranca los servicios de verdad en una carpeta temporal y usa los puertos 80, 443, 9070 y 3307.

La guía de arquitectura está en `.claude/skills/servidor-local-tauri/SKILL.md`.
