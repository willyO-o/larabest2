<?php
// Generado por {{ app_name }}. No editar: se regenera al cambiar www/.
// Carga Adminer y permite entrar como root sin contraseña: la base de datos
// solo escucha en 127.0.0.1.

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
    }

    return new LocalAdminer();
}

include "{{ adminer_php }}";
