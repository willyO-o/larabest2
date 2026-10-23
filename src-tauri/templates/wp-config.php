<?php
/**
 * Configuración de WordPress creada por {{ app_name }} al crear el sitio.
 * Puedes editar este archivo: no se regenera.
 */

// Base de datos local (MariaDB de {{ app_name }}, usuario root).
define( 'DB_NAME', '{{ db_name }}' );
define( 'DB_USER', 'root' );
define( 'DB_PASSWORD', '{{ db_password }}' );
define( 'DB_HOST', '127.0.0.1:{{ db_port }}' );
define( 'DB_CHARSET', 'utf8mb4' );
define( 'DB_COLLATE', '' );

// LARABEST: red local. Abierto desde otro equipo por IP (Menú → www →
// Compartir en la red local), las URLs siguen a esa dirección en lugar de
// la del dominio .test, que solo existe en este equipo.
if ( isset( $_SERVER['HTTP_HOST'] ) && preg_match( '/^\d{1,3}(\.\d{1,3}){3}(:\d+)?$/', $_SERVER['HTTP_HOST'] ) ) {
    define( 'WP_HOME', 'http://' . $_SERVER['HTTP_HOST'] );
    define( 'WP_SITEURL', 'http://' . $_SERVER['HTTP_HOST'] );
}

// Claves únicas generadas para este sitio.
define( 'AUTH_KEY',         '{{ keys[0] }}' );
define( 'SECURE_AUTH_KEY',  '{{ keys[1] }}' );
define( 'LOGGED_IN_KEY',    '{{ keys[2] }}' );
define( 'NONCE_KEY',        '{{ keys[3] }}' );
define( 'AUTH_SALT',        '{{ keys[4] }}' );
define( 'SECURE_AUTH_SALT', '{{ keys[5] }}' );
define( 'LOGGED_IN_SALT',   '{{ keys[6] }}' );
define( 'NONCE_SALT',       '{{ keys[7] }}' );

$table_prefix = 'wp_';

define( 'WP_DEBUG', false );

if ( ! defined( 'ABSPATH' ) ) {
	define( 'ABSPATH', __DIR__ . '/' );
}

require_once ABSPATH . 'wp-settings.php';
