use super::app_service::_AppService;
use toni::http_helpers::{Body, HttpRequest};
use toni_macros::{controller, delete, get, post, put};

#[controller(
  "/app",
  pub struct _AppController {
    app_service: _AppService,
  }
)]
impl _AppController {
    #[post("")]
    fn _create(&self, _req: HttpRequest) -> Body {
        let create: String = self.app_service.create();
        Body::Text(create)
    }

    #[get("")]
    fn _find_all(&self, _req: HttpRequest) -> Body {
        let find_all: String = self.app_service.find_all();
        Body::Text(find_all)
    }

    #[put("")]
    fn _update(&self, _req: HttpRequest) -> Body {
        let update: String = self.app_service.update();
        Body::Text(update)
    }

    #[delete("")]
    fn _delete(&self, _req: HttpRequest) -> Body {
        let delete: String = self.app_service.delete();
        Body::Text(delete)
    }
}
