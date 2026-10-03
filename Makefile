# Verificación del worker.
#
# La trampa de este proyecto: `ocr` arrastra tesseract y leptonica, que son
# librerías del sistema y no están en una máquina de desarrollo cualquiera. O
# sea que `cargo check` local NO cubre todo el código, y es exactamente así como
# se coló una función borrada que `extract_text_from_image` seguía usando: las
# cuatro combinaciones locales pasaron y la imagen no compiló.
#
#     make verificar     ← lo que hay que correr antes de commitear

SP ?= $(CURDIR)
PDFIUM_PATH ?= $(shell ls $(HOME)/.cache/pdfium/lib/libpdfium.dylib 2>/dev/null)

.PHONY: verificar local docker test

verificar: local test docker
	@echo "✅ compila en las 5 combinaciones y los tests pasan"

## Las cuatro combinaciones que SÍ compilan sin librerías del sistema.
local:
	@for f in "" "--features timbre" "--features pdf" "--features pdf,timbre"; do \
		printf "  cargo check %-26s" "$$f"; \
		cargo check $$f --quiet 2>&1 | grep -E "^error" && exit 1 || echo "ok"; \
	done

test:
	@printf "  cargo test                            "
	@PDFIUM_PATH=$(PDFIUM_PATH) cargo test --quiet --features "pdf,timbre" 2>&1 \
		| grep -E "FAILED|^error" && exit 1 || echo "ok"

## La quinta: `ocr`, que sólo compila donde están tesseract y leptonica.
## Se construye la imagen porque es el único entorno que las tiene.
docker:
	@printf "  docker build (--features ocr,timbre)  "
	@cd ../../.. && docker compose -f docker-compose-app-services.yml \
		build worker-storage-processor >/dev/null 2>&1 && echo "ok" \
		|| { echo "FALLÓ"; cd ../../..; docker compose -f docker-compose-app-services.yml \
		     build worker-storage-processor 2>&1 | grep -E "^[0-9.]+ error" | head -20; exit 1; }
