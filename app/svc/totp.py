import base64
import hashlib
import json
import secrets

import pyotp
from cryptography.fernet import Fernet

import bcrypt as _bcrypt

from app.security import SECRET_KEY

# Derive a Fernet key from TODOER_SECRET_KEY (which is a hex string).
# Fernet requires a 32-byte url-safe base64-encoded key.
_fernet_key = base64.urlsafe_b64encode(
    hashlib.sha256(SECRET_KEY.encode()).digest()
)
_fernet = Fernet(_fernet_key)

ISSUER = "Todoer"
BACKUP_CODE_COUNT = 10


def generate_secret() -> str:
    """Generate a new TOTP secret."""
    return pyotp.random_base32()


def encrypt_secret(secret: str) -> str:
    """Encrypt a TOTP secret for storage at rest."""
    return _fernet.encrypt(secret.encode()).decode()


def decrypt_secret(encrypted: str) -> str:
    """Decrypt a TOTP secret from storage."""
    return _fernet.decrypt(encrypted.encode()).decode()


def get_provisioning_uri(secret: str, email: str) -> str:
    """Get the otpauth:// URI for QR code generation."""
    totp = pyotp.TOTP(secret)
    return totp.provisioning_uri(name=email, issuer_name=ISSUER)


def verify_code(secret: str, code: str) -> bool:
    """Verify a TOTP code, allowing 1 window of drift."""
    totp = pyotp.TOTP(secret)
    return totp.verify(code, valid_window=1)


def generate_backup_codes() -> list[str]:
    """Generate plaintext backup codes (8 hex chars each)."""
    return [secrets.token_hex(4).upper() for _ in range(BACKUP_CODE_COUNT)]


def hash_backup_codes(codes: list[str]) -> str:
    """Hash backup codes and return as JSON array."""
    hashed = [
        _bcrypt.hashpw(c.encode(), _bcrypt.gensalt()).decode()
        for c in codes
    ]
    return json.dumps(hashed)


def verify_backup_code(code: str, hashed_codes_json: str) -> str | None:
    """
    Verify a backup code against the stored hashes.
    Returns updated JSON (with used code removed) or None if invalid.
    """
    hashed_codes = json.loads(hashed_codes_json)
    for i, h in enumerate(hashed_codes):
        if _bcrypt.checkpw(code.upper().encode(), h.encode()):
            hashed_codes.pop(i)
            return json.dumps(hashed_codes)
    return None
