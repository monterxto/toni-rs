use super::resource_name_service::_RESOURCE_NAME_SERVICE;
use toni::http_helpers::{Body, HttpRequest};
use toni_macros::{controller, delete, get, post, put};

#[controller(
  "/resource_name",
  pub struct _RESOURCE_NAME_CONTROLLER {
    resource_name_service: _RESOURCE_NAME_SERVICE,
  }
)]
impl _RESOURCE_NAME_CONTROLLER {
    #[post("")]
    fn _create(&self, _req: HttpRequest) -> Body {
        let create: String = self.resource_name_service.create();
        Body::text(create)
    }

    #[get("")]
    fn _find_all(&self, _req: HttpRequest) -> Body {
        let find_all: String = self.resource_name_service.find_all();
        Body::text(find_all)
    }

    #[get("/{id}")]
    fn _find_by_id(&self, req: HttpRequest) -> Body {
        let id = req.path_params.get("id").unwrap().parse::<i32>().unwrap();
        let find_by_id: String = self.resource_name_service.find_by_id(id);
        Body::text(find_by_id)
    }

    #[put("")]
    fn _update(&self, _req: HttpRequest) -> Body {
        let update: String = self.resource_name_service.update();
        Body::text(update)
    }

    #[delete("")]
    fn _delete(&self, _req: HttpRequest) -> Body {
        let delete: String = self.resource_name_service.delete();
        Body::text(delete)
    }
}
