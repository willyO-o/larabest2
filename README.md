# APP_NAME

Servidor local para Windows 10 y 11, inspirado en Laragon: gestiona **Nginx**, **PHP** y **MariaDB** como procesos propios y crea un dominio `<carpeta>.test` por cada proyecto de `www/`. Hecho con Tauri 2, Rust y Vue 3.

> El nombre definitivo está pendiente. Mientras tanto se usa el marcador `APP_NAME` (`productName` en `src-tauri/tauri.conf.json`). Al cambiarlo, cambia también la carpeta raíz por defecto (`C:\<nombre>`).

**Fase 1:** iniciar y detener todos los servicios o uno a la vez, cambiar la versión de PHP, crear un sitio `.test` por carpeta, vivir en la bandeja del sistema, y los botones Web, Base de datos, Terminal y Raíz.

## Carpeta de datos

Por defecto es `C:\APP_NAME`. Se puede cambiar en **Menú → Preferencias** (el cambio se aplica al reiniciar). La ruta no puede tener espacios porque Nginx y PHP fallan con ellas.

```text
C:\APP_NAME\
  bin\            binarios copiados a mano (ver abajo)
  www\            una subcarpeta = un sitio <carpeta>.test
  etc\            configuración generada (no editar; se regenera)
    nginx\custom\ tus inclusiones por sitio: <sitio>.conf (nunca se regeneran)
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
| Nginx | <https://nginx.org/en/download.html> | *Stable version* → `nginx/Windows-1.28.x` (zip) | `bin\nginx\1.28.0\` | `nginx.exe` y `conf\` |
| PHP 8.3 | <https://windows.php.net/download/> | PHP 8.3 → **VS16 x64 Non Thread Safe** (zip) | `bin\php\8.3.x\` | `php-cgi.exe` |
| PHP 8.4 | <https://windows.php.net/download/> | PHP 8.4 → **VS17 x64 Non Thread Safe** (zip) | `bin\php\8.4.x\` | `php-cgi.exe` |
| MariaDB 11.4 | <https://mariadb.org/download/> | MariaDB Server 11.4 (LTS), *MS Windows (64-bit)*, tipo **ZIP file** | `bin\mariadb\11.4.x\` | `bin\mariadbd.exe` |
| Adminer | <https://www.adminer.org/#download> | *Adminer for MySQL* (`adminer-x.y.z-mysql.php`) | `bin\adminer\` renombrado a **`adminer.php`** | `adminer.php` |
| Composer (opcional) | <https://getcomposer.org/download/> | `composer.phar` | `bin\composer\composer.phar` | `composer.phar` |

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
3. Descomprímelo y ejecuta `APP_NAME_0.1.0_x64-setup.exe`.
4. El instalador no está firmado, así que SmartScreen avisará: pulsa **Más información → Ejecutar de todas formas**. Se instala solo para tu usuario y no pide administrador.
5. Si la PC no tiene WebView2 (en Windows 10 es raro), el instalador lo descarga. Sin internet, compila con `webviewInstallMode: offlineInstaller`.

Para desinstalar: **Configuración → Aplicaciones → APP_NAME → Desinstalar**. La carpeta `C:\APP_NAME` (proyectos y bases de datos) no se borra.

## Pruebas manuales en Windows 10

Prepara `bin\` como se describe arriba y crea `www\blog\index.php` con `<?php phpinfo();` y `www\tienda\public\index.php` con `<?php echo "Laravel";`.

**Arranque y bandeja**

- [ ] Al abrir aparece la ventana compacta con Nginx, PHP y MariaDB en *Detenido* y con su versión y puerto.
- [ ] Aparece un aviso de UAC para el archivo hosts (los sitios `blog.test`, `tienda.test` y `adminer.test`). Al aceptarlo, `C:\Windows\System32\drivers\etc\hosts` tiene un bloque `# >>> APP_NAME` y el resto del archivo no cambió.
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
- [ ] **Raíz** abre `C:\APP_NAME` en el Explorador.

**Errores visibles**

- [ ] Con otro programa usando el puerto 80 (por ejemplo IIS, XAMPP o `python -m http.server 80`), **Iniciar todo** deja Nginx en *Error* con "El puerto 80 está ocupado por …" en su fila, y PHP y MariaDB arrancan igual.
- [ ] Sin el runtime de Visual C++, PHP queda en *Error* con un mensaje que lo nombra.
- [ ] Si matas `mariadbd.exe` desde el Administrador de tareas, MariaDB pasa a *Error* ("terminó inesperadamente") y no se reinicia solo.

**Salida**

- [ ] **Salir** desde la bandeja detiene los servicios (MariaDB de forma ordenada) y cierra la app.
- [ ] Si la app se cierra a la fuerza (Administrador de tareas → Finalizar tarea), Nginx, PHP y MariaDB también se cierran.

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

En Linux se compila y se prueba el núcleo (dominio, drivers y casos de uso) con adaptadores falsos. La app no gestiona servicios fuera de Windows. Para revisar el código de Windows desde Linux: `rustup target add x86_64-pc-windows-msvc` y `cargo clippy --target x86_64-pc-windows-msvc --all-targets`.

La guía de arquitectura está en `.claude/skills/servidor-local-tauri/SKILL.md`.
