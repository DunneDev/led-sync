import os
import azure.functions as func
import time
import urllib.parse
import hmac
import hashlib
import base64

app = func.FunctionApp(http_auth_level=func.AuthLevel.FUNCTION)


@app.post(route="generate_sas_token")
def generate_sas_token(req):
    ttl = int(time.time() + req.params.get("expiry"))
    uri = req.params.get("uri")
    sign_key = f"{urllib.parse.quote(uri)}\n{ttl}".encode("utf-8")

    signature = base64.b64encode(
        hmac.new(
            base64.b64decode(req.params.get("key")),
            sign_key,
            hashlib.sha256
        ).digest()
    )

    token = (
        f"SharedAccessSignature sr={urllib.parse.quote(uri)}"
        f"&sig={urllib.parse.quote(signature)}"
        f"&se={ttl}"
        f"&skn={req.params.get("policy_name")}"
    )

    return token
