use toni_macros::module;

use super::app_controller::*;
use super::app_service::*;

#[module(
  imports: [],
  controllers: [_AppController],
  providers: [_AppService],
  exports: []
)]
impl AppModule {}
