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
    fn _create(&self) -> Body {
        let create: String = self.app_service.create();
        Body::text(create)
    }

    #[get("")]
    fn _find_all(&self) -> Body {
        let find_all: String = self.app_service.find_all();
        Body::text(find_all)
    }

    #[put("")]
    fn _update(&self) -> Body {
        let update: String = self.app_service.update();
        Body::text(update)
    }

    #[delete("")]
    fn _delete(&self) -> Body {
        let delete: String = self.app_service.delete();
        Body::text(delete)
    }
}
