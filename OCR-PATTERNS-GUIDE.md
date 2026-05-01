# Guía de Patrones OCR para Facturas DTE Chilenas

## Descripción General
El servicio `DocumentManagerService` extrae **4 campos clave** de facturas DTE chilenas usando OCR con Tesseract y patrones regex.

---

## Campos Extraídos

### 1. **Número de Factura** (`numero_factura`)
**Patrón Regex:**
```regex
(?i)\bN[º°o]?\s*([0-9]{1,8})\b
```

**Explicación:**
- `(?i)` - Case insensitive
- `\bN` - Palabra que inicia con "N"
- `[º°o]?` - Opcional: símbolo º, °, o la letra o
- `\s*` - Espacio opcional
- `([0-9]{1,8})` - Captura 1 a 8 dígitos del número
- `\b` - Límite de palabra

**Ejemplos que coinciden:**
- `Nº 123456`
- `N° 44`
- `No 999999`
- `N 555`
- `N o 12345`

**Resultado:** `123456`, `44`, `999999`, etc.

---

### 2. **RUT del Deudor** (`rut_deudor`)
**Patrón Regex (contexto):**
```regex
(?i)(?:DEUDOR|SEÑOR(?:ES)?(?:\s*\(ES\))?|NOMBRE)\s*(?::|\.)*\s*(\d{1,2}\.\d{3}\.\d{3}-[\dkK])
```

**Patrón Regex (fallback - primer RUT encontrado):**
```regex
(\d{1,2}\.\d{3}\.\d{3}-[\dkK])
```

**Explicación:**
- Busca **en contexto** de palabras como `DEUDOR`, `SEÑOR(ES)`, `SEÑORES (ES)` o `NOMBRE`
- Si no encuentra en contexto, retorna el **primer RUT** que coincida
- Formato: `XX.XXX.XXX-K` donde K es dígito (0-9) o letra verificadora (k/K)

**Ejemplos:**
- `DEUDOR: 12.345.678-9` → Extrae `12.345.678-9`
- `SEÑOR: 12.345.678-K` → Extrae `12.345.678-K`
- `SEÑORES (ES): 12.345.678-5` → Extrae `12.345.678-5`

---

### 3. **Nombre del Deudor** (`nombre_deudor`)
**Patrones Regex (en orden de búsqueda):**

```regex
(?i)SEÑOR(?:ES)?(?:\s*\(ES\))?:\s*([^:\n]+?)(?:\n|$|RUT|rut)
```
```regex
(?i)SEÑORES\s*\(ES\):\s*([^:\n]+?)(?:\n|$|RUT|rut)
```
```regex
(?i)NOMBRE\s*(?:DEL)?(?:\s+DEUDOR)?:\s*([^:\n]+?)(?:\n|$|RUT|rut)
```

**Explicación:**
- Busca después de `SEÑOR(ES):`, `SEÑORES (ES):`, o `NOMBRE (DEL DEUDOR):`
- Captura todo hasta el final de línea o la palabra "RUT"
- Requiere mínimo 3 caracteres válidos

**Ejemplos:**
- `SEÑOR: JUAN PÉREZ GARCIA` → Extrae `JUAN PÉREZ GARCIA`
- `SEÑORES (ES): EMPRESA ABC LTDA` → Extrae `EMPRESA ABC LTDA`
- `NOMBRE DEL DEUDOR: MARÍA RODRÍGUEZ` → Extrae `MARÍA RODRÍGUEZ`

---

### 4. **Monto Total** (`monto_total`)
**Patrones Regex (en orden de búsqueda):**

```regex
(?i)TOTAL\s+[S$5s8]\s*([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?)
```
```regex
(?i)TOTAL\s*:\s*[S$5s8]\s*([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?)
```
```regex
(?i)MONTO\s+TOTAL\s+[S$5s8]\s*([0-9]{1,3}(?:[.,][0-9]{3})*(?:[.,][0-9]{2})?)
```

**Explicación:**
- Busca después de `TOTAL`, `TOTAL:` o `MONTO TOTAL`
- El símbolo `$` (peso) puede ser leído por Tesseract como: `S`, `s`, `5`, `8`
- Captura números con formato: `999`, `999.999`, `999,999.99`, etc.
- Soporta tanto `.` como `,` como separador de decimales/miles

**Ejemplos:**
- `TOTAL $ 1.250.500` → Extrae `1.250.500`
- `TOTAL: $ 999,99` → Extrae `999,99`
- `TOTAL 5 50000` (OCR confunde $ con 5) → Extrae `50000`
- `TOTAL S 1250500` (OCR confunde $ con S) → Extrae `1250500`
- `MONTO TOTAL $ 100.000,00` → Extrae `100.000,00`

---

## Flujo de Procesamiento

```
PDF → Renderizado (3000px ancho)
  ↓
Dividido en 4 franjas con overlap
  ↓
Cada franja:
  - Conversión a escala de grises
  - Binarización dual (threshold 150 y 130)
  - OCR General (Tesseract PSM 6)
  - OCR Focalizado (Tesseract PSM 7, whitelist reducido)
  - Normalización de variantes "Nº"
  ↓
Aplicar patrones regex a texto consolidado
  ↓
Retornar InvoiceData struct con campos extraídos
```

---

## Salida (Estructura JSON)

```json
{
  "numero_factura": "123456",
  "rut_deudor": "12.345.678-9",
  "nombre_deudor": "JUAN PÉREZ GARCÍA",
  "monto_total": "1.250.500",
  "extraction_timestamp": "2026-04-23T13:56:51Z"
}
```

---

## Depuración

### Debug Images
Durante OCR, se generan imágenes temporales en `/tmp/`:
- `ocr_debug_page{n}_tile{m}_orig.png` - Imagen original de la franja
- `ocr_debug_page{n}_tile{m}_bw.png` - Imagen binarizada (threshold 150)
- `ocr_debug_page{n}_tile{m}_bw_thin.png` - Imagen binarizada (threshold 130)

### Logs
Buscar en los logs del container por:
- `"Datos de factura extraídos exitosamente"` - Confirmación de extracción
- `"Patrón Nº detectado"` - Detección de número de factura
- `"Tile {n}/{m}"` - Progreso de procesamiento

---

## Configuración Tesseract

### Pass General (PSM 6 - Block Mode)
```
user_defined_dpi: 300
preserve_interword_spaces: 1
tessedit_char_whitelist: ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789Nnº°.-:/,()@ 
```

### Pass Focalizado (PSM 7 - Line Mode)
```
user_defined_dpi: 300
preserve_interword_spaces: 1
tessedit_pageseg_mode: 7
tessedit_char_whitelist: Nnº°oO0123456789
```

---

## Limitaciones y Consideraciones

1. **Resolución:** Se renderiza a 3000px de ancho para mejor OCR (requiere más CPU/tiempo)
2. **Idioma:** Soporta `spa+eng` por defecto, configurable por `OCR_LANGUAGE`
3. **Confusiones OCR comunes:**
   - `$` → `S`, `s`, `5`, `8`
   - `º` → `°`, `o`, `O`
   - `N` (letra) → Bien identificado, pero requiere whitelist
   - Números similares: `0` vs `O`, `1` vs `l` (parcialmente manejado)

4. **Orden de búsqueda:** Los patrones se aplican en orden. El primero que coincida es utilizado.
5. **RUT de cliente vs Deudor:** Se prioriza RUT en contexto de palabras clave para diferenciar.

---

## Próximas Mejoras

- [ ] Thresholds adaptativos por región (vs. fijos 150/130)
- [ ] Detección de múltiples deudores (facturas colectivas)
- [ ] Validación de dígito verificador del RUT
- [ ] Limpieza automática de `/tmp/` después de OCR exitoso
- [ ] Storage de logs de OCR por auditoría (texto bruto vs. esperado)

