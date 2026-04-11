from azure.messaging.webpubsubservice import WebPubSubServiceClient
import os
import azure.functions as func

app = func.FunctionApp(http_auth_level=func.AuthLevel.FUNCTION)


@app.route(route="get_access_token")
def get_access_token(req: func.HttpRequest) -> func.HttpResponse:
    service = WebPubSubServiceClient.from_connection_string(
        os.environ["WPS_CONNECTION_STRING"], hub="led"
    )

    token = service.get_client_access_token(user_id="esp32-device")

    return func.HttpResponse(token["url"], mimetype="text/plain")
