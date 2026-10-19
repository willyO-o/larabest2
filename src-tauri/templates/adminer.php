<?php
// Generado por {{ app_name }}. No editar: se regenera al cambiar www/.
// Carga Adminer y permite entrar como root sin escribir la contraseña: la
// base de datos solo escucha en 127.0.0.1.

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
{%- if password %}

        // root de MariaDB o MySQL tiene contraseña: se completa sola.
        function credentials(): array
        {
            $credentials = parent::credentials();
            if ($credentials[1] === 'root' && (string) $credentials[2] === '') {
                $credentials[2] = '{{ password }}';
            }
            return $credentials;
        }
{%- endif %}
    }

    return new LocalAdminer();
}

include "{{ adminer_php }}";
