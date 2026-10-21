/* Funciones de las DLL de compatibilidad para Windows 7 (ver los .def).
 * Se compilan sin la librería de C. */

/* FALSE / S_OK: "no se adjuntaron detalles del error". */
__int64 larabest_zero(void) { return 0; }

/* E_NOTIMPL, para las que devuelven datos por un puntero: quien llama ve
 * el error y no lee una salida sin rellenar. */
long larabest_notimpl(void) { return (long)0x80004001L; }
