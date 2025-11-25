use crate::{
    http_helpers::{HttpRequest, HttpResponse, IntoResponse},
    traits_helpers::validate::Validatable,
};

#[derive(Debug)]
pub struct Context {
    original_request: HttpRequest,
    response: Option<Box<dyn IntoResponse<Response = HttpResponse> + Send>>,
    should_abort: bool,
    dto: Option<Box<dyn Validatable>>,
}

impl Context {
    pub fn from_request(req: HttpRequest) -> Self {
        Self {
            original_request: req,
            response: None,
            should_abort: false,
            dto: None,
        }
    }

    pub fn take_request(&self) -> &HttpRequest {
        &self.original_request
    }

    pub fn set_response(
        &mut self,
        response: Box<dyn IntoResponse<Response = HttpResponse> + Send>,
    ) {
        self.response = Some(response);
    }

    pub fn get_response(self) -> Box<dyn IntoResponse<Response = HttpResponse> + Send> {
        if let Some(response) = self.response {
            return response;
        }

        panic!("Response not set in context");

        //  else {
        //     HttpResponse::InternalServerError().body("Internal Server Error")
        // }
    }

    pub fn get_response_ref(&self) -> Option<&(dyn IntoResponse<Response = HttpResponse> + Send)> {
        self.response.as_deref()
    }

    pub fn get_response_mut(&mut self) -> &mut (dyn IntoResponse<Response = HttpResponse> + Send) {
        if let Some(response_box) = self.response.as_mut() {
            return response_box.as_mut();
        }

        panic!("Response not set in context");
    }

    pub fn abort(&mut self) {
        self.should_abort = true;
    }

    pub fn should_abort(&self) -> bool {
        self.should_abort
    }

    pub fn set_dto(&mut self, dto: Box<dyn Validatable>) {
        self.dto = Some(dto);
    }

    pub fn get_dto(&self) -> Option<&dyn Validatable> {
        self.dto.as_deref()
    }
}
