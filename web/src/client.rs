use {
    gloo_utils::window,
    glove::Resp,
    merman_api::{
        API_PATH,
        api::ReqTrait,
    },
    wasm_bindgen::{
        JsCast,
        JsValue,
    },
    wasm_bindgen_futures::JsFuture,
    web_sys::{
        Request,
        RequestInit,
        Response,
    },
};

pub async fn client_send<I: ReqTrait>(req: I) -> Result<I::Resp, String> {
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(&serde_json::to_string(&req.to_enum()).unwrap()));
    let request =
        Request::new_with_str_and_init(
            API_PATH,
            &init,
        ).map_err(|e| js_error("Error building the api request", e))?;
    let response =
        JsFuture::from(window().fetch_with_request(&request))
            .await
            .map_err(|e| js_error("Error sending the api request", e))?;
    let response: Response = response.dyn_into().unwrap();
    let body =
        JsFuture::from(response.text().map_err(|e| js_error("Error reading the api response", e))?)
            .await
            .map_err(|e| js_error("Error reading the api response", e))?;
    let body = body.as_string().unwrap_or_default();
    if !response.ok() {
        return Err(format!("The editor's server answered {}: {}", response.status(), body));
    }
    match serde_json::from_str::<Resp<I::Resp>>(&body) {
        Ok(Resp::Ok(v)) => return Ok(v),
        Ok(Resp::Err(e)) => return Err(e),
        Err(e) => return Err(format!("Error parsing the api response: {}\nBody: {}", e, body)),
    }
}

fn js_error(context: &str, e: JsValue) -> String {
    return format!("{}: {:?}", context, e);
}
