# LARABEST

Servidor local para Windows 10 y 11 y para macOS 13 o posterior, inspirado en Laragon: gestiona **Nginx** (o **Apache**), **PHP** y **MariaDB** como procesos propios y crea un dominio `<carpeta>.test` por cada proyecto de `www/`. Hecho con Tauri 2, Rust y Vue 3.

La interfaz sigue la estructura de Laragon (ventana compacta, una fila por servicio, cinco botones abajo y un menú contextual en árbol) con identidad propia. La banda superior cambia de color con el estado: grafito detenido, ámbar iniciando, verde todo en marcha, rojo con un fallo.

**Fase 1:** iniciar y detener todos los servicios o uno a la vez, cambiar la versión de PHP, crear un sitio `.test` por carpeta, vivir en la bandeja del sistema, y los botones Web, Base de datos, Terminal y Raíz.

**Fase 2:** crear un sitio en blanco, WordPress o Laravel con un clic, HTTPS opcional con una autoridad certificadora local, acceso a configuraciones y logs desde el menú, y tema Sistema/Claro/Oscuro.

**Fase 3:** la misma app en macOS, con Nginx, PHP (`php-fpm`) y MariaDB instalados con Homebrew. Ver [macOS](#macos).

**Servicios opcionales:** PostgreSQL, Redis, MongoDB y Mailpit, en Windows y macOS. Ver [Servicios opcionales](#servicios-opcionales).

**Apache:** se puede usar Apache en lugar de Nginx, con los mismos sitios. Ver [Apache en lugar de Nginx](#apache-en-lugar-de-nginx).

**Agregar desde el menú:** versiones de PHP, Nginx, Apache, MariaDB, MySQL y los servicios opcionales, más Adminer, phpMyAdmin, Composer, Node.js y Git, con un clic. Ver [Agregar programas desde el menú](#agregar-programas-desde-el-menú).

**Paridad con Laragon:** MySQL en lugar de MariaDB, contraseña de root, puertos y dominio desde la ventana, iniciar con el sistema y minimizado, atajos de línea de comandos y opción de agregarlos al PATH, Procfile, plantillas propias de *Crear sitio* y respaldos de las bases de datos. Ver [Paridad con Laragon](#paridad-con-laragon).

## Menú contextual

Clic derecho en cualquier parte de la ventana, el botón **Menú** o el ícono de la bandeja abren el mismo menú:

| Menú | Contenido |
| --- | --- |
| Iniciar todo / Detener todo | según el estado |
| www | un sitio por carpeta (abre `http://<carpeta>.test`), **Crear sitio** (en blanco, WordPress, Laravel y las plantillas de `etc\sitios.conf`), abrir la carpeta www, actualizar sitios y archivo hosts |
| Nginx o Apache | el servidor web elegido: versión, recargar configuración, carpeta de configuración propia por sitio (`etc\nginx\custom\<sitio>.conf` o `etc\apache\custom\<sitio>.conf`), configuración generada, logs |
| PHP | versión, `php.ini` de ajustes propios, extensiones (marcar o desmarcar), `php.ini` generado, log de errores |
| MariaDB o MySQL | el motor elegido: versión, `my.cnf` de ajustes propios, Adminer, phpMyAdmin, **Contraseña de root…**, respaldar ahora, restaurar un respaldo, carpeta de respaldos, `my.cnf` generado, carpeta de datos, log de errores |
| PostgreSQL, Redis, MongoDB, Mailpit | uno por cada servicio opcional en la pila (ver abajo) |
| Logs | errores y accesos del servidor web, errores de PHP y de la base de datos, logs de los opcionales, carpeta de logs |
| Herramientas | Web, Base de datos, Correo (con Mailpit), Terminal, carpeta raíz, **Procfile** (estado de cada proceso, editar, reiniciar) |
| Agregar | descargar versiones y herramientas (ver abajo) |
| Preferencias | tema (Sistema, Claro, Oscuro), servidor web (Nginx o Apache), base de datos (MariaDB o MySQL), servicios opcionales, botón Base de datos (Adminer o phpMyAdmin), **Puertos y dominio…**, iniciar con Windows (o macOS), iniciar minimizado en la bandeja, iniciar todo al abrir, agregar php, composer, mysql… al PATH, respaldar las bases de datos al detener, HTTPS para los sitios `.test`, carpeta de certificados, cambiar la carpeta raíz, abrir `app.json` |

Lo generado (`etc\nginx\nginx.conf`, `etc\php\php.ini`, `etc\mariadb\my.cnf`) se regenera en cada inicio: ábrelo solo para consultarlo. Tus ajustes van en archivos que nunca se regeneran:

- PHP: `etc\php\custom\user.ini` (PHP lo carga con `PHP_INI_SCAN_DIR` y tiene prioridad sobre el generado).
- MariaDB: `etc\mariadb\custom\user.cnf` (incluido con `!includedir`).
- Nginx: `etc\nginx\custom\<sitio>.conf` (incluido dentro del `server` de ese sitio).

Las extensiones marcadas en el menú se guardan en `app.json` y PHP se reinicia solo si estaba en marcha. Al cambiar de versión, se reinicia solo ese servicio.

## Agregar programas desde el menú

**Menú → Agregar** instala con un clic (como el *Quick add* de Laragon):

| Paquete | Windows | macOS |
| --- | --- | --- |
| PHP 8.4 y 8.3 | última versión de la rama desde `windows.php.net` (NTS x64) | `brew install php@8.4` / `php@8.3` |
| Nginx | versión estable de `nginx.org` | `brew install nginx` |
| Apache | build de Apache Lounge para Windows x64, desde su réplica en `fossies.org` (la misma que usa Scoop; `apachelounge.com` bloquea algunas descargas automáticas) | `brew install httpd` |
| MariaDB 11.4 | última 11.4 de `archive.mariadb.org` | `brew install mariadb@11.4` |
| MySQL 8.4 (LTS) | zip de `cdn.mysql.com` (la versión la da la fórmula `mysql@8.4` de `formulae.brew.sh`) | `brew install mysql@8.4` |
| PostgreSQL 17 | binarios de EDB (`get.enterprisedb.com`) | `brew install postgresql@17` |
| Redis | `redis-windows/redis-windows` (GitHub) | `brew install redis` |
| MongoDB | `fastdl.mongodb.org` | tap `mongodb/brew` y `brew install mongodb-community` |
| Mailpit | `axllent/mailpit` (GitHub) | `brew install mailpit` |
| Adminer, phpMyAdmin, Composer | sus descargas oficiales, en `bin\` | igual |
| Node.js (LTS) | zip de `nodejs.org` en `bin\node\<versión>\` | `brew install node` |
| Git | MinGit de `git-for-windows` (GitHub) en `bin\git\<versión>\` | `brew install git` |

En Windows cada versión queda en su carpeta (`bin\php\8.4.13\`), así que conviven varias y se elige en **Menú → PHP → Versión**. El menú muestra al lado lo que ya está instalado; volver a agregar algo baja la versión más nueva de esa rama si la hay. El progreso aparece en la línea de estado y, al terminar, la fila del servicio ya muestra la versión nueva. En macOS hace falta tener Homebrew (<https://brew.sh>); la app solo ejecuta `brew install`.

## Paridad con Laragon

### MySQL en lugar de MariaDB

**Menú → Agregar → MySQL 8.4** y luego **Menú → Preferencias → Base de datos → MySQL**. Si MariaDB estaba en marcha, se detiene y MySQL arranca en su lugar; la fila cambia de MariaDB a MySQL y el submenú de la base de datos pasa a llamarse MySQL. Se vuelve igual a MariaDB.

- Mismo puerto (3306 por defecto), usuario `root` y botón Base de datos. Los sitios WordPress y Laravel nuevos usan el motor elegido (`DB_CONNECTION=mysql` en el `.env`).
- Cada motor tiene sus datos: `data\mariadb` y `data\mysql`, y su configuración: `etc\mariadb\my.cnf` y `etc\mysql\my.cnf` (generados), con los ajustes propios en `etc\<motor>\custom\user.cnf`. Cambiar de motor no mueve bases de datos: usa un respaldo y restáuralo en el otro.
- La primera vez MySQL crea sus tablas con `mysqld --initialize-insecure` (root sin contraseña). El protocolo X (puerto 33060) queda apagado.
- Logs: `logs\mysql-error.log`.

### Contraseña de root

**Menú → MariaDB (o MySQL) → Contraseña de root…** abre un panel en la ventana. La contraseña se cambia en todos los usuarios `root` del motor (`localhost`, `127.0.0.1`…) y se guarda en `app.json`, porque la app la necesita para detener el motor, comprobar que responde, crear bases de datos y respaldar. Adminer y phpMyAdmin siguen entrando solos (su configuración generada lleva la contraseña). Los sitios WordPress y Laravel nuevos la reciben en `wp-config.php` o `.env`; los creados antes conservan la anterior y hay que cambiarla a mano. Cada motor tiene la suya. Déjala vacía para quitarla.

Solo se aceptan letras, números y `_-.@!%*+=?,:` (sin espacios, comillas ni `$` o `#`), para que no haya que escaparla en `wp-config.php`, `.env` ni la configuración de phpMyAdmin.

### Puertos y dominio

**Menú → Preferencias → Puertos y dominio…** abre un panel con los puertos de HTTP, HTTPS, PHP, la base de datos, PostgreSQL, Redis, MongoDB y Mailpit, y el dominio de los sitios (`test` por defecto, por ejemplo `local`). La app rechaza puertos repetidos (contando los 4 de PHP en Windows) y los dominios `dev` y `app` (los navegadores los obligan a HTTPS). Al guardar se regeneran los sitios, el archivo hosts y los certificados, y se reinicia solo lo que estaba en marcha y cambió. Los `.env` de Laravel ya creados conservan su `APP_URL` y `DB_PORT`.

### Iniciar con el sistema y minimizado

- **Preferencias → Iniciar con Windows** (o **con macOS**): la app se abre al entrar a la sesión, en la bandeja y sin mostrar la ventana. En Windows queda en *Configuración → Aplicaciones → Inicio* (clave `Run` del usuario, sin permisos de administrador); en macOS como un elemento de inicio (`LaunchAgent`).
- **Preferencias → Iniciar minimizado en la bandeja**: cada vez que se abre, la ventana espera al clic en la bandeja.
- Junto con **Iniciar todo al abrir**, los servicios quedan en marcha al prender la PC, como en Laragon.

### Terminal, Node.js, Git y el PATH

La **Terminal** pone delante del PATH el PHP activo, el cliente de MariaDB o MySQL, el servidor web, Node.js y Git de la app (los de **Agregar**), solo para esa consola.

Además, en `etc\shell\` la app genera atajos que apuntan siempre a la versión activa: `php` (con el `php.ini` de la app), `composer`, `mariadb`/`mariadb-dump` o `mysql`/`mysqldump`, `node`, `npm`, `npx` y `git` (`.bat` en Windows, scripts en macOS). Se regeneran al cambiar de versión o de motor y al agregar programas.

**Preferencias → Agregar php, composer, mysql… al PATH** agrega solo esa carpeta al PATH de tu usuario, así esos comandos funcionan en cualquier consola, en VS Code o en PowerShell. Es lo único que la app escribe fuera de su carpeta (además del archivo hosts) y solo si lo eliges; desmarcarlo lo quita. En Windows se escribe en `HKCU\Environment` sin tocar las demás entradas; en macOS se agrega un bloque marcado a `~/.zprofile` (y a `~/.bash_profile` si existe). Abre una consola nueva para verlo. Algunas extensiones de VS Code piden la ruta de `php.exe` y no un `.bat`: usa `C:\LARABEST\bin\php\<versión>\php.exe`.

### Procfile

Como en Laragon, `C:\LARABEST\Procfile` define procesos propios que arrancan con **Iniciar todo** y se detienen con **Detener todo**. Una línea por proceso, `nombre: comando`; el comando corre en `www\` con el mismo PATH que la Terminal y su salida queda en `logs\procfile-<nombre>.log`:

```text
cola: cd tienda && php artisan queue:work
vite: cd tienda && npm run dev
reverb: cd tienda && php artisan reverb:start
```

**Menú → Herramientas → Procfile** muestra cuáles están en marcha y permite editarlo (la primera vez se crea con estos ejemplos comentados) o reiniciar los procesos. Si la app se cierra a la fuerza, al volver a abrirla detiene los que quedaron.

### Plantillas propias de Crear sitio

`etc\sitios.conf` (como el `sites.conf` de Laragon) agrega plantillas a **Crear sitio**, en el formulario (**Otra…**) y en **Menú → www → Crear sitio**. Una por línea, `Nombre=acción`, con `%s` como nombre del sitio:

```text
Symfony=composer create-project symfony/skeleton %s
CodeIgniter 4=composer create-project codeigniter4/appstarter %s
CakePHP=composer create-project --prefer-dist cakephp/app %s
Drupal=composer create-project drupal/recommended-project %s
MiProyecto=git clone https://github.com/usuario/proyecto.git %s
MiPlantilla=https://example.com/plantilla.zip
```

Un comando de Composer usa el PHP y el Composer de la app (se descarga si falta); una URL de `.zip` se descomprime en `www\<sitio>`; cualquier otro comando corre en `www\` con el PATH de la Terminal. También se crea una base de datos con el nombre del sitio (si el motor puede arrancar). Las cuatro primeras vienen activas; **Editar plantillas** abre el archivo.

### Respaldos

- **Menú → MariaDB (o MySQL) → Respaldar las bases de datos ahora**: un `.sql` por base (sin las del sistema) con `mariadb-dump` o `mysqldump` (`--single-transaction`, rutinas, eventos y disparadores) en `backups\<fecha>-<motor>\`.
- **Preferencias → Respaldar las bases de datos al detener**: lo mismo antes de detener el motor (con su botón, **Detener todo** o al salir). Si falla, el motor se detiene igual y el error queda en `logs\backups.log`.
- Se conservan los 10 respaldos más recientes (`backupKeep` en `app.json`; 0 los conserva todos).
- **Restaurar un respaldo (.sql)…** ejecuta un `.sql` con el cliente del motor, después de confirmar: los respaldos de la app traen su `CREATE DATABASE`, así que reemplazan las tablas de esas bases.

## Apache en lugar de Nginx

**Menú → Preferencias → Servidor web → Apache** (antes, **Menú → Agregar → Apache**). Si el servidor web estaba en marcha, Nginx se detiene y Apache arranca en su lugar; la fila de la ventana cambia de Nginx a Apache. Se vuelve igual a Nginx.

Todo lo demás es igual: los mismos sitios `<carpeta>.test`, puertos 80 y 443, HTTPS con los mismos certificados, Adminer y phpMyAdmin, y PHP por FastCGI (los 4 `php-cgi` en Windows, repartidos con `mod_proxy_balancer`; `php-fpm` en macOS). Con Apache funcionan los `.htaccess` de WordPress y Laravel (`AllowOverride All` y `mod_rewrite`).

- Configuración generada: `etc\apache\httpd.conf` y un `etc\apache\sites\<sitio>.conf` por sitio (no editar). Tus ajustes por sitio van en `etc\apache\custom\<sitio>.conf`, incluido dentro de su `<VirtualHost>`.
- Logs: `logs\apache-error.log` y `logs\apache-access.log` (**Menú → Logs**).
- **Recargar** (al crear o borrar un sitio, o desde el menú): en macOS es `httpd -k graceful`, sin cortar las visitas. En Windows Apache corre como aplicación de consola y no se puede recargar desde otro proceso (`httpd -k` solo controla el servicio de Windows): la app lo reinicia, que tarda un segundo.
- Windows: las compilaciones recientes de Apache Lounge (VS18) piden el **runtime de Visual C++ más reciente** (<https://aka.ms/vs/17/release/vc_redist.x64.exe> instala la última versión 14.x). Si falta o es viejo, la fila de Apache lo dice.
- macOS: el Apache que trae el sistema puede estar ocupando el puerto 80 (`sudo apachectl stop`).

## Base de datos: Adminer o phpMyAdmin

El botón **Base de datos** abre Adminer en `http://adminer.test` (con MariaDB como root sin contraseña). Con phpMyAdmin copiado en `bin\phpmyadmin\`, se sirve en `http://phpmyadmin.test` y entra solo como root; en **Menú → Preferencias → Botón Base de datos** se elige cuál abre el botón (**Menú → MariaDB** tiene los dos). La app genera `bin\phpmyadmin\config.inc.php` (phpMyAdmin solo lo lee de su carpeta); tus ajustes van en `etc\phpmyadmin\custom.inc.php`.

## Servicios opcionales

Además de la pila base (Nginx, PHP y MariaDB), la app gestiona cuatro servicios opcionales. Cada uno aparece como una fila más (la ventana crece) y arranca con **Iniciar todo** en cuanto está instalado; en **Menú → Preferencias → Servicios** se puede quitar o volver a poner en la pila. Al quitarlo, si estaba en marcha, se detiene.

| Servicio | Puerto | Para qué | Windows (en `bin\<servicio>\<versión>\`) | macOS |
| --- | --- | --- | --- | --- |
| PostgreSQL | 5432 | otra base de datos (Laravel puede usarla) | zip de binarios de <https://www.enterprisedb.com/download-postgresql-binaries>: la carpeta `pgsql` pasa a ser `bin\postgresql\17.6\` (la app busca `bin\postgres.exe`) | `brew install postgresql@17` |
| Redis | 6379 | caché, sesiones y colas | no hay versión oficial; la de <https://github.com/redis-windows/redis-windows/releases> (`redis-server.exe` en la raíz de la carpeta) | `brew install redis` |
| MongoDB | 27017 | base de datos de documentos | zip de <https://www.mongodb.com/try/download/community> (Windows x64, *zip*): `bin\mongodb\8.0.x\bin\mongod.exe` | `brew tap mongodb/brew`, `brew trust mongodb/brew` y `brew install mongodb/brew/mongodb-community` |
| Mailpit | 1025 (SMTP) y 8025 (web) | atrapa el correo que envían los sitios | `mailpit-windows-amd64.zip` de <https://github.com/axllent/mailpit/releases>: `bin\mailpit\1.27.x\mailpit.exe` | `brew install mailpit` |

- **PostgreSQL**: la primera vez se crea el clúster en `data\postgresql` con `initdb` (UTF-8, usuario `postgres` sin contraseña, solo en 127.0.0.1). Sus ajustes propios van en `data\postgresql\postgresql.conf` (**Menú → PostgreSQL**); la app solo fija el puerto y las direcciones. **Menú → PostgreSQL → Abrir Adminer** entra como `postgres`. En Windows PHP necesita la extensión `pdo_pgsql` (márcala en **Menú → PHP → Extensiones**); el PHP de Homebrew ya la trae.
- **Redis**: ajustes propios en `etc\redis\custom\user.conf`. Guarda los datos en `data\redis` al detenerse.
- **MongoDB**: datos en `data\mongodb`. Para usarlo desde PHP hace falta la extensión `mongodb` (en Windows, `php_mongodb.dll` de <https://pecl.php.net/package/mongodb> en `ext\`; en macOS `pecl install mongodb`).
- **Mailpit**: con Mailpit en la pila, `mail()` de PHP (WordPress) y los sitios Laravel nuevos (`MAIL_MAILER=smtp`, `MAIL_PORT=1025`) envían todo a Mailpit, y el correo se ve en **Menú → Mailpit → Abrir el correo recibido** (`http://127.0.0.1:8025`). Nada sale a internet.

Al detener, MariaDB, PostgreSQL y Redis cierran de forma ordenada con sus propias herramientas; MongoDB y Mailpit reciben la señal de cierre (Ctrl+C en Windows) y solo se fuerzan si no terminan en 10 s.

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
    mysql\custom\   user.cnf con tus ajustes de MySQL (nunca se regenera)
    shell\         atajos php, composer, mysql, node, git… (los que van al PATH)
    sitios.conf    plantillas propias de Crear sitio
  data\mariadb\   bases de datos (data\mysql\ con MySQL)
  backups\        respaldos de las bases de datos, uno por fecha
  Procfile        procesos propios que arrancan con Iniciar todo
  logs\           logs de Nginx, PHP y MariaDB
  tmp\            PID y temporales
  app.json        versión activa de cada servicio, puertos
```

La app crea todas estas carpetas la primera vez que se abre.

## Preparar la carpeta `bin\`

Lo más simple es **Menú → Agregar** (ver arriba). También se pueden copiar a mano. Cada versión va en su propia carpeta, con el número de versión como nombre. La app la detecta si encuentra el ejecutable indicado.

| Servicio | De dónde | Qué descargar | Carpeta destino | La app busca |
| --- | --- | --- | --- | --- |
| Nginx | <https://nginx.org/en/download.html> | *Stable version* → `nginx/Windows-1.28.x` (zip) | `bin\nginx\1.28.0\` | `nginx.exe` |
| PHP 8.3 | <https://windows.php.net/download/> | PHP 8.3 → **VS16 x64 Non Thread Safe** (zip) | `bin\php\8.3.x\` | `php-cgi.exe` |
| PHP 8.4 | <https://windows.php.net/download/> | PHP 8.4 → **VS17 x64 Non Thread Safe** (zip) | `bin\php\8.4.x\` | `php-cgi.exe` |
| MariaDB 11.4 | <https://mariadb.org/download/> | MariaDB Server 11.4 (LTS), *MS Windows (64-bit)*, tipo **ZIP file** | `bin\mariadb\11.4.x\` | `bin\mariadbd.exe` |
| Apache (opcional) | <https://www.apachelounge.com/download/> | `httpd-2.4.x-…-Win64-VS18.zip` | el contenido de su carpeta `Apache24` en `bin\apache\2.4.x\` | `bin\httpd.exe` |
| Adminer | <https://www.adminer.org/#download> | *Adminer* (`adminer-x.y.z.php`, sirve para MariaDB y PostgreSQL) | `bin\adminer\` renombrado a **`adminer.php`** | `adminer.php` |
| phpMyAdmin (opcional) | <https://www.phpmyadmin.net/downloads/> | *all-languages* (zip) | el contenido de la carpeta en `bin\phpmyadmin\` | `index.php` |
| Composer (opcional: se descarga solo al crear un sitio Laravel) | <https://getcomposer.org/download/> | `composer.phar` | `bin\composer\composer.phar` | `composer.phar` |

Pasos:

1. Descomprime cada zip y renombra la carpeta resultante con el número de versión. Por ejemplo, `php-8.4.6-nts-Win32-vs17-x64` pasa a ser `bin\php\8.4.6\`, y `mariadb-11.4.5-winx64` pasa a ser `bin\mariadb\11.4.5\`.
2. Comprueba que el ejecutable quede directamente dentro de la carpeta de versión (`bin\php\8.4.6\php-cgi.exe`) y no en una subcarpeta extra.
3. PHP usa builds **NTS x64** (Non Thread Safe). Las *Thread Safe* no sirven con `php-cgi`.
4. PHP, MariaDB y Apache necesitan el **runtime de Visual C++ x64** reciente: <https://aka.ms/vs/17/release/vc_redist.x64.exe> instala el último. Si falta o es antiguo, la fila del servicio lo dice.
5. No hace falta tocar `php.ini`, `nginx.conf` ni `my.cnf`: la app los genera en `etc\`.

Resultado esperado:

```text
bin\
  nginx\1.28.0\nginx.exe
  php\8.3.20\php-cgi.exe
  php\8.4.6\php-cgi.exe
  mariadb\11.4.5\bin\mariadbd.exe
  apache\2.4.69\bin\httpd.exe  (opcional)
  adminer\adminer.php
  phpmyadmin\index.php        (opcional)
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

**Servicios opcionales**

- [ ] Copia Redis en `bin\redis\<versión>\`: al reabrir el menú (o la app) aparece la fila Redis y la ventana crece. **Iniciar todo** lo arranca.
- [ ] **Menú → Preferencias → Servicios → Redis** lo quita de la pila: se detiene y su fila desaparece. Volver a marcarlo la trae de vuelta.
- [ ] Con PostgreSQL en `bin\postgresql\17.x\`: la primera vez crea `data\postgresql`; **Menú → PostgreSQL → Abrir Adminer** entra como `postgres` (con `pdo_pgsql` marcada en PHP).
- [ ] Con MongoDB: arranca, y **Detener** lo cierra sin forzarlo (el log `logs\mongodb.log` muestra un cierre normal, sin «unclean shutdown» al volver a iniciarlo).
- [ ] Con Mailpit: un sitio WordPress nuevo envía el correo de «¿Olvidaste tu contraseña?» y aparece en **Menú → Mailpit → Abrir el correo recibido**.

**Agregar desde el menú**

- [ ] **Menú → Agregar → PHP 8.4**: la línea de estado muestra el progreso (buscar versión, descargar, extraer) y al terminar la fila de PHP y **Menú → PHP → Versión** tienen la versión nueva. El menú Agregar la marca como instalada.
- [ ] **Agregar → MongoDB** (o cualquier servicio opcional) hace aparecer su fila.
- [ ] Con la ventana oculta, agregar desde la bandeja termina con un diálogo.

**Apache**

- [ ] **Menú → Agregar → Apache** y luego **Preferencias → Servidor web → Apache** con todo en marcha: la fila Nginx cambia por Apache, *En marcha*, y en el Administrador de tareas hay dos `httpd.exe` y ningún `nginx.exe`.
- [ ] `http://blog.test` y `http://tienda.test` funcionan igual; en Laravel, `http://tienda.test/up` responde (pasa por el `.htaccess`). En WordPress, **Ajustes → Enlaces permanentes → Nombre de la entrada** funciona (crea su `.htaccess`).
- [ ] Crear `www\nuevo\index.php` con Apache en marcha: `http://nuevo.test` funciona en un par de segundos (Apache se reinicia solo).
- [ ] Con HTTPS activo, `https://blog.test` abre con candado.
- [ ] **Detener** cierra los dos `httpd.exe` sin forzarlos (en `logs\apache-error.log` no hay errores de cierre).
- [ ] **Servidor web → Nginx** vuelve atrás.

**Paridad con Laragon**

- [ ] **Agregar → MySQL 8.4** y **Preferencias → Base de datos → MySQL** con todo en marcha: MariaDB se detiene, la fila MySQL queda *En marcha* y el menú se llama MySQL. **Nuevo sitio → Laravel** deja `DB_CONNECTION=mysql` y migra.
- [ ] **MySQL → Contraseña de root…** con `Clave-1`: el botón Base de datos sigue entrando a Adminer y phpMyAdmin sin pedir nada; **Detener** y **Iniciar** funcionan. Un WordPress nuevo instala sin pedir datos de la base. Vaciarla quita la contraseña.
- [ ] **Preferencias → Puertos y dominio…**: HTTP 8080 y dominio `local` con Nginx en marcha: solo Nginx se reinicia y `http://blog.local:8080` funciona. Un puerto repetido o el dominio `dev` muestran el error en el panel.
- [ ] **Preferencias → Iniciar con Windows** y **Iniciar minimizado**: tras cerrar sesión y volver a entrar, el ícono está en la bandeja y la ventana no se abre. Aparece en *Configuración → Aplicaciones → Inicio*.
- [ ] **Agregar → Node.js** y **Agregar → Git**: en la **Terminal**, `node -v`, `npm -v` y `git --version` funcionan.
- [ ] **Preferencias → Agregar php, composer, mysql… al PATH**: en una consola nueva de PowerShell, fuera de la app, `php -v`, `composer -V` y `mariadb --version` usan la versión activa; al cambiar de PHP en el menú, `php -v` cambia sin tocar el PATH. Desmarcarlo lo quita.
- [ ] **Herramientas → Procfile → Editar Procfile**, agrega `srv: php -S 127.0.0.1:8000 -t blog` y **Iniciar todo**: `http://127.0.0.1:8000` responde y el submenú dice «srv: en marcha». **Detener todo** lo detiene.
- [ ] **Nuevo sitio → Otra… → Symfony** con el nombre `api`: Composer crea `www\api` y `http://api.test` muestra la página de Symfony.
- [ ] **MariaDB → Respaldar las bases de datos ahora**: aparece `backups\<fecha>-mariadb\` con un `.sql` por base. Borra una tabla en Adminer y **Restaurar un respaldo** la trae de vuelta.

## macOS

En macOS la app usa los programas de **Homebrew** en vez de copiarlos a `bin/`. Funciona en Apple Silicon y en Intel (el `.dmg` es universal) desde macOS 13 Ventura; Homebrew ofrece binarios listos para macOS 14 o posterior (en versiones anteriores, y en Intel para algunas fórmulas, los compila y tarda bastante).

### Instalar los servicios

1. Instala Homebrew si no lo tienes: <https://brew.sh> (pide la contraseña de administrador una vez).
2. En la Terminal:

   ```bash
   brew install nginx php@8.3 php@8.4 mariadb@11.4
   ```

   Opcional: `brew install httpd` para usar Apache en lugar de Nginx, y los servicios opcionales (`postgresql@17`, `redis`, `mailpit`, y MongoDB con `brew tap mongodb/brew`, `brew trust mongodb/brew` y `brew install mongodb/brew/mongodb-community`). También se instalan desde **Menú → Agregar**.

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
- [ ] **Menú → Agregar → Redis** ejecuta `brew install redis` (la línea de estado muestra el progreso) y aparece la fila Redis.
- [ ] Con `brew install httpd`, **Preferencias → Servidor web → Apache**: los sitios siguen funcionando y crear una carpeta nueva en `www/` la sirve sin cortar las visitas (`httpd -k graceful`).

El CI de macOS ya prueba, con los programas reales de Homebrew: arranque de la pila con los servicios opcionales, un sitio servido por Nginx y php-fpm, el correo a Mailpit, la base de datos con root sin contraseña, Adminer, la creación de un sitio WordPress y uno Laravel, MongoDB cerrado en orden, HTTPS, PostgreSQL, la Terminal, el cambio de versión de PHP, el cambio a Apache (con su recarga) y la limpieza de procesos huérfanos. Lo que no prueba (necesita una persona frente al Mac): los avisos de contraseña, la ventana, la barra de menús y abrir la app descargada.

El CI de Windows (job `servicios`) agrega todo el catálogo desde **Menú → Agregar** con las descargas oficiales y recorre lo mismo con Nginx y luego con Apache, salvo PostgreSQL (el runner corre como administrador y `postgres.exe` se niega a arrancar así; en una PC normal no pasa).

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

En un Mac con Homebrew, la prueba de punta a punta (`cargo test --lib -- --ignored --nocapture e2e_homebrew`) arranca los servicios de verdad en una carpeta temporal y usa los puertos 80, 443, 9070, 3307, 5433, 6380, 27018, 1026 y 8026. En Windows, `e2e_windows` descarga todo el catálogo y usa 8080 y 8443 en lugar de 80 y 443.

La guía de arquitectura está en `.claude/skills/servidor-local-tauri/SKILL.md`.
