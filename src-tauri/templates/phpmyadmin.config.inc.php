<?php
// Generado por {{ app_name }}. No editar: se regenera al actualizar los sitios.
// Tus ajustes van en {{ custom }} (se incluye al final).
// Entra como root con la contraseña de la app: MariaDB solo escucha en 127.0.0.1.

$cfg['blowfish_secret'] = '{{ secret }}';

$i = 1;
$cfg['Servers'][$i]['host'] = '127.0.0.1';
$cfg['Servers'][$i]['port'] = '{{ port }}';
$cfg['Servers'][$i]['auth_type'] = 'config';
$cfg['Servers'][$i]['user'] = 'root';
$cfg['Servers'][$i]['password'] = '{{ password }}';
$cfg['Servers'][$i]['AllowNoPassword'] = true;

$cfg['TempDir'] = '{{ tmp }}';
$cfg['DefaultLang'] = 'es';
$cfg['VersionCheck'] = false;

if (is_file('{{ custom }}')) {
    include '{{ custom }}';
}
