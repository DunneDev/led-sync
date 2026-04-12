import os
import azure.functions as func
import time
import urllib.parse
import hmac
import hashlib
import base64

app = func.FunctionApp(http_auth_level=func.AuthLevel.FUNCTION)


@app.route(route="generate_sas_token")
def generate_sas_token(req):
    ttl = int(time.time() + 3600)
    uri = f"{os.getenv("IOT_HUB_NAME")}.azure-devices.net/{os.getenv("DEVICE_NAME")}/?api-version=2021-04-12"
    sign_key = f"{urllib.parse.quote(uri)}\n{ttl}".encode("utf-8")

    signature = base64.b64encode(
        hmac.new(
            base64.b64decode(os.getenv("IOT_HUB_KEY")),
            sign_key,
            hashlib.sha256
        ).digest()
    )

    token = (
        f"SharedAccessSignature sr={urllib.parse.quote(uri)}"
        f"&sig={urllib.parse.quote(signature.decode())}"
        f"&se={ttl}"
        f"&skn=iothubowner"
    )

    return token
