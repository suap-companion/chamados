#!/usr/bin/env python3
"""Gera `protegido.pdf`: um PDF de uma página protegido por senha de abertura, para os testes automáticos.

Usa a criptografia padrão do PDF (Standard Security Handler, revisão 2: RC4 de 40 bits, a mais simples e a
que qualquer leitor entende), escrita só com a biblioteca padrão do Python, sem dependências.

    senha de abertura (usuário): senha
    senha do dono:               dono
    texto da página:             "Documento protegido por senha."

Rode da raiz do repositório:  python -I scripts/gerar-pdf-protegido.py

Escreve o mesmo arquivo em dois lugares (o conteúdo é determinístico):
  crates/chamados-mobile/testdata/protegido.pdf        (testes Rust, via include_bytes!)
  android/app/src/androidTest/assets/protegido.pdf     (testes de instrumentação do app)

Para conferir com o poppler:  pdftotext -upw senha protegido.pdf -   (lê)   |   pdftotext protegido.pdf -   (recusa)
"""
import hashlib
import pathlib
import struct

USER_PASSWORD = b"senha"
OWNER_PASSWORD = b"dono"
TEXT = "Documento protegido por senha."
PADDING = bytes.fromhex("28BF4E5E4E758A4164004E56FFFA01082E2E00B6D0683E802F0CA9FE6453697A")
FILE_ID = bytes.fromhex("00112233445566778899AABBCCDDEEFF")
PERMISSIONS = -4  # tudo permitido, no formato do PDF
KEY_LENGTH = 5  # 40 bits


def rc4(key: bytes, data: bytes) -> bytes:
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) % 256
        s[i], s[j] = s[j], s[i]
    out = bytearray()
    i = j = 0
    for byte in data:
        i = (i + 1) % 256
        j = (j + s[i]) % 256
        s[i], s[j] = s[j], s[i]
        out.append(byte ^ s[(s[i] + s[j]) % 256])
    return bytes(out)


def pad(password: bytes) -> bytes:
    return (password + PADDING)[:32]


def owner_entry() -> bytes:
    """Algoritmo 3 (revisão 2): o /O."""
    key = hashlib.md5(pad(OWNER_PASSWORD)).digest()[:KEY_LENGTH]
    return rc4(key, pad(USER_PASSWORD))


def encryption_key(owner: bytes) -> bytes:
    """Algoritmo 2 (revisão 2): a chave de criptografia do arquivo."""
    data = pad(USER_PASSWORD) + owner + struct.pack("<i", PERMISSIONS) + FILE_ID
    return hashlib.md5(data).digest()[:KEY_LENGTH]


def user_entry(key: bytes) -> bytes:
    """Algoritmo 4 (revisão 2): o /U."""
    return rc4(key, PADDING)


def object_key(key: bytes, number: int, generation: int = 0) -> bytes:
    """Algoritmo 1: a chave de cada objeto."""
    data = key + struct.pack("<I", number)[:3] + struct.pack("<I", generation)[:2]
    return hashlib.md5(data).digest()[: min(len(key) + 5, 16)]


def hex_string(data: bytes) -> bytes:
    return b"<" + data.hex().upper().encode() + b">"


def build() -> bytes:
    owner = owner_entry()
    key = encryption_key(owner)
    user = user_entry(key)

    content_plain = f"BT /F1 14 Tf 72 760 Td ({TEXT}) Tj ET".encode("latin-1")
    objects = {}
    objects[1] = b"<< /Type /Catalog /Pages 2 0 R >>"
    objects[2] = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"
    objects[3] = (
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] "
        b"/Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
    )
    encrypted = rc4(object_key(key, 4), content_plain)
    objects[4] = b"<< /Length %d >>\nstream\n" % len(encrypted) + encrypted + b"\nendstream"
    objects[5] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"
    objects[6] = (
        b"<< /Filter /Standard /V 1 /R 2 /O " + hex_string(owner) + b" /U " + hex_string(user)
        + b" /P %d >>" % PERMISSIONS
    )

    body = b"%PDF-1.4\n"
    offsets = {}
    for number in sorted(objects):
        offsets[number] = len(body)
        body += b"%d 0 obj\n" % number + objects[number] + b"\nendobj\n"
    xref = len(body)
    size = max(objects) + 1
    body += b"xref\n0 %d\n0000000000 65535 f \n" % size
    for number in range(1, size):
        body += b"%010d 00000 n \n" % offsets[number]
    body += (
        b"trailer\n<< /Size %d /Root 1 0 R /Encrypt 6 0 R /ID [" % size
        + hex_string(FILE_ID) + hex_string(FILE_ID) + b"] >>\nstartxref\n%d\n%%%%EOF\n" % xref
    )
    return body


def main() -> None:
    data = build()
    for destination in (
        pathlib.Path("crates/chamados-mobile/testdata/protegido.pdf"),
        pathlib.Path("android/app/src/androidTest/assets/protegido.pdf"),
    ):
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        print(f"{destination} ({len(data)} bytes)")


if __name__ == "__main__":
    main()
