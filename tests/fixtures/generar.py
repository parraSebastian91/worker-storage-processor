#!/usr/bin/env python3
"""Genera los fixtures de timbre que usan los tests.

Reproduce lo que hace el facturador del cedente: par RSA, CAF con esa clave,
DD firmado y PDF417 en byte mode. Los datos son inventados — los fixtures se
versionan, así que no pueden salir de facturas reales (esas llevan RUT, razón
social y monto de terceros, y además un TED real NO se puede anonimizar: el
FRMT firma el DD completo, así que cambiarle un campo rompe la firma).

    pip install cryptography pdf417gen pillow
    python3 generar.py

Estructura del CAF: la REAL, medida sobre facturas de cuatro facturadores
distintos. La clave pública viaja como módulo y exponente en <RSAPK>, no como
PEM en <RSAPUBK> — ese tag vive en el archivo CAF que entrega el SII, dentro de
<AUTORIZACION>, y NO en el timbre.
"""
import base64
import pathlib
import re

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding, rsa
from pdf417gen import encode, render_image

AQUI = pathlib.Path(__file__).parent


def compactar(dd: str) -> str:
    """Lo que se firma: el DD sin whitespace entre tags."""
    return re.sub(r">\s+<", "><", dd)


def b64_de_entero(n: int) -> str:
    return base64.b64encode(n.to_bytes((n.bit_length() + 7) // 8, "big")).decode()


def armar_ted(folio: int, rsr: str, mnt: int, it1: str, td: int = 33, pretty: bool = True):
    """`pretty=False` deja el DD compacto, que es como viaja en las facturas
    reales medidas. Con `pretty=True` lleva CRLF entre tags, para ejercitar el
    camino de compactación."""
    # Exponente 3, como los CAF reales del SII.
    llave = rsa.generate_private_key(public_exponent=3, key_size=1024)
    nums = llave.public_key().public_numbers()

    caf = (
        '<CAF version="1.0"><DA>'
        "<RE>76543210-K</RE><RS>EMISOR DE PRUEBA SpA</RS>"
        f"<TD>{td}</TD><RNG><D>1</D><H>1000</H></RNG><FA>2026-01-01</FA>"
        f"<RSAPK><M>{b64_de_entero(nums.n)}</M><E>{b64_de_entero(nums.e)}</E></RSAPK>"
        "<IDK>300</IDK></DA>"
        '<FRMA algoritmo="SHA1withRSA">firmaDelSiiQueEstaFaseNoVerifica</FRMA></CAF>'
    )
    interior = (
        "<RE>76543210-K</RE>"
        f"<TD>{td}</TD>"
        f"<F>{folio}</F>"
        "<FE>2026-06-14</FE>"
        "<RR>77777777-7</RR>"
        f"<RSR>{rsr}</RSR>"
        f"<MNT>{mnt}</MNT>"
        f"<IT1>{it1}</IT1>"
        f"{caf}"
        "<TSTED>2026-06-14T10:30:00</TSTED>"
    )
    # Así viaja en el timbre: "pretty", con CRLF entre tags.
    dd = ("<DD>\r\n" + interior.replace("><", ">\r\n<") + "\r\n</DD>"
          if pretty else f"<DD>{interior}</DD>")
    firma = llave.sign(compactar(dd).encode("ISO-8859-1"), padding.PKCS1v15(), hashes.SHA1())
    frmt = base64.b64encode(firma).decode()
    return f'<TED version="1.0">{dd}<FRMT algoritmo="SHA1withRSA">{frmt}</FRMT></TED>'


def guardar(nombre: str, ted: str):
    datos = ted.encode("ISO-8859-1")
    (AQUI / f"{nombre}.ted.txt").write_bytes(datos)
    codes = encode(datos, columns=12, security_level=5, encoding="iso-8859-1")
    render_image(codes, scale=3, ratio=3, padding=12).save(AQUI / f"{nombre}.png")
    altos = sum(1 for b in datos if b > 0x7F)
    print(f"  {nombre:24} {len(datos):5} bytes, {altos} sobre 0x7F")


def guardar_con_simbolo_corrupto(nombre: str, ted: str, de: str, a: str):
    """Firma el TED correcto pero imprime el símbolo con un carácter cambiado.

    Reproduce un defecto REAL medido sobre el corpus: un facturador cuyo encoder
    no cambia a modo byte ante un carácter fuera del ASCII imprimible emite en su
    lugar uno de la tabla de puntuación de PDF417 —se midió un `º` (0xBA) que
    salió como `]` (0x5D)— mientras firma el texto correcto. Ese timbre no
    verifica tal cual para nadie, y acusarlo de firma inválida sería acusar de
    adulterada a una factura legítima.
    """
    (AQUI / f"{nombre}.ted.txt").write_bytes(ted.encode("ISO-8859-1"))
    datos = ted.replace(de, a).encode("ISO-8859-1")
    codes = encode(datos, columns=12, security_level=5, encoding="iso-8859-1")
    render_image(codes, scale=3, ratio=3, padding=12).save(AQUI / f"{nombre}.png")
    print(f"  {nombre:24} {len(datos):5} bytes, símbolo con {de!r} → {a!r}")


def guardar_pdf_vectorial(nombre: str, ted: str):
    """Dibuja el timbre como rectángulos en un PDF, con el defecto real medido.

    Hay facturadores que no embeben el PDF417 como imagen sino que pintan cada
    módulo oscuro como su propio rectángulo. Y lo pintan **más angosto que su
    celda**: medido, módulos de 0,374 pt de ancho ubicados cada 0,5025. Esa
    ranura blanca hace que una corrida de módulos vecinos se imprima como barras
    separadas, y PDF417 codifica justamente en el ancho de las corridas — así que
    el símbolo impreso no decodifica a NINGUNA resolución.

    Acá se reproduce exacto: el bitmap de 1 px por módulo dice qué celdas van
    oscuras, y cada una se dibuja angosta y separada.
    """
    from reportlab.pdfgen import canvas

    datos = ted.encode("ISO-8859-1")
    (AQUI / f"{nombre}.ted.txt").write_bytes(datos)

    codes = encode(datos, columns=12, security_level=5, encoding="iso-8859-1")
    matriz = render_image(codes, scale=1, ratio=1, padding=0).convert("L")
    cols, filas = matriz.size

    PASO_X, ANCHO = 0.5025, 0.374      # el módulo se dibuja más angosto que su celda
    PASO_Y, ALTO = 1.008, 0.878
    X0, Y0 = 60.0, 500.0

    c = canvas.Canvas(str(AQUI / f"{nombre}.pdf"), pagesize=(612, 792))
    c.setFillColorRGB(0, 0, 0)
    px = matriz.load()
    oscuros = 0
    for f in range(filas):
        for x in range(cols):
            if px[x, f] < 128:
                c.rect(X0 + x * PASO_X, Y0 - f * PASO_Y, ANCHO, ALTO, stroke=0, fill=1)
                oscuros += 1
    # Algo de ruido alrededor, como tiene cualquier factura: líneas de formulario
    # que NO son módulos y que la detección tiene que descartar.
    c.rect(50, 700, 500, 0.8, stroke=0, fill=1)
    c.rect(50, 650, 500, 0.8, stroke=0, fill=1)
    c.save()
    print(f"  {nombre+'.pdf':24} {cols}x{filas} módulos, {oscuros} rectángulos")


if __name__ == "__main__":
    # Caso feliz, con ñ y acentos: los bytes que se corrompen si se lee UTF-8.
    ok = armar_ted(128, "PEÑALOLÉN DISTRIBUCIÓN LTDA", 1250500,
                   "Señalización y montaje, instalación básica")
    guardar("valido", ok)

    # Mismo timbre con el monto cambiado DESPUÉS de firmar: la firma ya no cuadra.
    guardar("adulterado", ok.replace("<MNT>1250500</MNT>", "<MNT>9999999</MNT>"))

    # DD compacto: la forma en que viaja en las facturas reales medidas.
    guardar("compacto", armar_ted(129, "CLIENTE SIN ACENTOS", 500000,
                                  "Servicio", pretty=False))

    # Documento no cedible: una boleta (39) no se puede ceder a un factoring.
    guardar("no_cedible", armar_ted(77, "CLIENTE FINAL", 11900, "Producto", td=39))
    # El símbolo imprime "]" donde el DD firmado dice "º": la firma sólo cuadra
    # si se repara el carácter, y que cuadre es la prueba de que se reparó bien.
    guardar_con_simbolo_corrupto(
        "simbolo_corrupto",
        armar_ted(640, "REGIMIENTO LOGISTICO Nº2", 981907, "Insumos"),
        "Nº2",
        "N]2",
    )

    # Timbre dibujado como rectángulos, con los módulos más angostos que su
    # celda: el defecto que ninguna resolución de render arregla.
    guardar_pdf_vectorial("vectorial", armar_ted(1726, "BESALCO PIQUES Y TUNELES S.A.", 252280,
                                                 "Monomando lavaplatos", pretty=False))

    print("\nlisto. Los .ted.txt son el original, para comparar byte a byte.")
