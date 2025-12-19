use toni_macros::module;

use super::resource_name_controller::*;
use super::resource_name_service::*;

#[module(
  imports: [],
  controllers: [_RESOURCE_NAME_CONTROLLER],
  providers: [_RESOURCE_NAME_SERVICE],
  exports: []
)]
impl RESOURCE_NAME_MODULE {}
