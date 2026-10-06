<?php
// Sitio creado por {{ app_name }}. Reemplaza este archivo por tu proyecto.
?>
<!doctype html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{{ name }}</title>
  <style>
    body { margin: 0; font: 16px/1.5 system-ui, "Segoe UI", sans-serif; color: #15171c; background: #f3f4f7; }
    main { max-width: 40rem; margin: 12vh auto; padding: 0 1.5rem; }
    h1 { margin: 0 0 .5rem; font-size: 2rem; }
    code { background: #e7e9ee; padding: .1rem .35rem; border-radius: 4px; }
  </style>
</head>
<body>
  <main>
    <h1>{{ name }}</h1>
    <p>El sitio funciona. Edita <code>index.php</code> en <code>{{ path }}</code> para empezar.</p>
    <p>PHP <?= PHP_VERSION ?></p>
  </main>
</body>
</html>
