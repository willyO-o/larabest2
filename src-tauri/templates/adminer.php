<?php
// Generado por {{ app_name }}. No editar: se regenera al cambiar www/.
// Carga Adminer y entra como root con la contraseña de la app (o sin
// contraseña): la base de datos solo escucha en 127.0.0.1.

function adminer_object()
{
    if (class_exists('Adminer\Adminer')) {
        class LocalAdminerBase extends \Adminer\Adminer {}
    } else {
        class LocalAdminerBase extends \Adminer {}
    }

    class LocalAdminer extends LocalAdminerBase
    {
        function login($login, $password)
        {
            return true;
        }
{% if password %}

        // MariaDB con la contraseña de root de la app; PostgreSQL entra sin
        // contraseña, como siempre.
        function credentials()
        {
            $server = defined('Adminer\SERVER') ? constant('Adminer\SERVER') : constant('SERVER');
            $password = isset($_GET['pgsql']) ? '' : '{{ password }}';
            return array($server, $_GET['username'], $password);
        }
{% endif %}
    }

    return new LocalAdminer();
}

include "{{ adminer_php }}";
