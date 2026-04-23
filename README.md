# worker-storage-processor

## OCR con pdfium-render + tesseract

La integracion OCR esta implementada y se activa con el feature `ocr`.

### Compilar sin OCR

```bash
cargo check
```

### Compilar con OCR

```bash
cargo check --features ocr
```

### Requisitos nativos en Windows

La compilacion con OCR requiere librerias nativas de Tesseract/Leptonica y Pdfium.

1. Instalar vcpkg y ejecutar:

```powershell
vcpkg integrate install
```

2. Instalar librerias:

```powershell
vcpkg install tesseract:x64-windows leptonica:x64-windows
```

3. Definir variable de entorno:

```powershell
setx VCPKG_ROOT "C:\ruta\a\vcpkg"
```

4. Asegurar que Pdfium este disponible para carga dinamica (PATH o instalacion del sistema).

### Idioma OCR

Por defecto se usa `spa+eng`. Se puede cambiar por variable de entorno:

```powershell
setx OCR_TESSERACT_LANG "spa+eng"
```
