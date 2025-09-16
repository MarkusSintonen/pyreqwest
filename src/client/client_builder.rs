use crate::client::Client;
use crate::client::client::{BaseClient, SyncClient};
use crate::client::internal::ConnectionLimiter;
use crate::client::runtime::Runtime;
use crate::client::runtime::RuntimeHandle;
use crate::http::internal::json::JsonHandler;
use crate::http::{CookieStore, CookieStorePyProxy};
use crate::http::{HeaderMap, Url, UrlType};
use crate::proxy::ProxyBuilder;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{PyTraverseError, PyVisit};
use pyo3_bytes::PyBytes;
use reqwest::redirect;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
#[pyclass(subclass)]
pub struct BaseClientBuilder {
    inner: Option<reqwest::ClientBuilder>,
    middlewares: Option<Vec<Py<PyAny>>>,
    json_handler: Option<JsonHandler>,
    max_connections: Option<usize>,
    total_timeout: Option<Duration>,
    pool_timeout: Option<Duration>,
    error_for_status: bool,
    default_headers: Option<HeaderMap>,
    runtime: Option<Py<Runtime>>,
    base_url: Option<Url>,
}

#[pyclass(extends=BaseClientBuilder)]
pub struct ClientBuilder;

#[pyclass(extends=BaseClientBuilder)]
pub struct SyncClientBuilder;

#[pymethods]
impl BaseClientBuilder {
    fn base_url(mut slf: PyRefMut<Self>, base_url: UrlType) -> PyResult<PyRefMut<Self>> {
        if !base_url.0.as_str().ends_with('/') {
            return Err(PyValueError::new_err("base_url must end with a trailing slash '/'"));
        }
        slf.check_inner()?;
        slf.base_url = Some(base_url.into());
        Ok(slf)
    }

    fn runtime(mut slf: PyRefMut<Self>, runtime: Py<Runtime>) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.runtime = Some(runtime);
        Ok(slf)
    }

    fn with_middleware(mut slf: PyRefMut<Self>, middleware: Py<PyAny>) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.middlewares.get_or_insert_with(Vec::new).push(middleware);
        Ok(slf)
    }

    #[pyo3(signature = (*, **kwargs))]
    fn json_handler<'py>(
        mut slf: PyRefMut<'py, Self>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<PyRefMut<'py, Self>> {
        slf.check_inner()?;
        let json_handler = slf.json_handler.get_or_insert_default();
        if let Some(kwargs) = kwargs {
            if kwargs.contains("loads")? {
                json_handler.set_loads(kwargs.get_item("loads")?);
            }
            if kwargs.contains("dumps")? {
                json_handler.set_dumps(kwargs.get_item("dumps")?);
            }
        }
        Ok(slf)
    }

    fn max_connections(mut slf: PyRefMut<Self>, max_connections: Option<usize>) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.max_connections = max_connections;
        Ok(slf)
    }

    fn error_for_status(mut slf: PyRefMut<Self>, value: bool) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.error_for_status = value;
        Ok(slf)
    }

    fn user_agent(slf: PyRefMut<Self>, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.user_agent(value)))
    }

    fn default_headers(mut slf: PyRefMut<'_, Self>, headers: HeaderMap) -> PyResult<PyRefMut<'_, Self>> {
        slf.check_inner()?;
        slf.default_headers = Some(headers);
        Ok(slf)
    }

    fn cookie_store(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.cookie_store(enable)))
    }

    fn cookie_provider(slf: PyRefMut<'_, Self>, provider: Py<CookieStore>) -> PyResult<PyRefMut<'_, Self>> {
        Self::apply(slf, |builder| Ok(builder.cookie_provider(Arc::new(CookieStorePyProxy(provider)))))
    }

    fn gzip(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.gzip(enable)))
    }

    fn brotli(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.brotli(enable)))
    }

    fn zstd(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.zstd(enable)))
    }

    fn deflate(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.deflate(enable)))
    }

    fn max_redirects(slf: PyRefMut<Self>, max_redirects: usize) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.redirect(redirect::Policy::limited(max_redirects))))
    }

    fn referer(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.referer(enable)))
    }

    fn proxy<'py>(slf: PyRefMut<'py, Self>, proxy: Bound<'_, ProxyBuilder>) -> PyResult<PyRefMut<'py, Self>> {
        let proxy = proxy.borrow_mut().build()?;
        Self::apply(slf, |builder| Ok(builder.proxy(proxy)))
    }

    fn no_proxy(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.no_proxy()))
    }

    fn timeout(mut slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.total_timeout = Some(timeout);
        Ok(slf)
    }

    fn read_timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.read_timeout(timeout)))
    }

    fn connect_timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.connect_timeout(timeout)))
    }

    fn pool_timeout(mut slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        slf.check_inner()?;
        slf.pool_timeout = Some(timeout);
        Ok(slf)
    }

    fn pool_idle_timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.pool_idle_timeout(timeout)))
    }

    fn pool_max_idle_per_host(slf: PyRefMut<Self>, max_idle: usize) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.pool_max_idle_per_host(max_idle)))
    }

    fn http1_title_case_headers(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http1_title_case_headers()))
    }

    fn http1_allow_obsolete_multiline_headers_in_responses(
        slf: PyRefMut<Self>,
        value: bool,
    ) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http1_allow_obsolete_multiline_headers_in_responses(value)))
    }

    fn http1_ignore_invalid_headers_in_responses(slf: PyRefMut<Self>, value: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http1_ignore_invalid_headers_in_responses(value)))
    }

    fn http1_allow_spaces_after_header_name_in_responses(slf: PyRefMut<Self>, value: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http1_allow_spaces_after_header_name_in_responses(value)))
    }

    fn http1_only(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http1_only()))
    }

    fn http09_responses(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http09_responses()))
    }

    fn http2_prior_knowledge(slf: PyRefMut<Self>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_prior_knowledge()))
    }

    fn http2_initial_stream_window_size(slf: PyRefMut<Self>, value: Option<u32>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_initial_stream_window_size(value)))
    }

    fn http2_initial_connection_window_size(slf: PyRefMut<Self>, value: Option<u32>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_initial_connection_window_size(value)))
    }

    fn http2_adaptive_window(slf: PyRefMut<Self>, enabled: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_adaptive_window(enabled)))
    }

    fn http2_max_frame_size(slf: PyRefMut<Self>, value: Option<u32>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_max_frame_size(value)))
    }

    fn http2_max_header_list_size(slf: PyRefMut<Self>, value: u32) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_max_header_list_size(value)))
    }

    fn http2_keep_alive_interval(slf: PyRefMut<Self>, value: Option<Duration>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_keep_alive_interval(value)))
    }

    fn http2_keep_alive_timeout(slf: PyRefMut<Self>, timeout: Duration) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_keep_alive_timeout(timeout)))
    }

    fn http2_keep_alive_while_idle(slf: PyRefMut<Self>, enabled: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.http2_keep_alive_while_idle(enabled)))
    }

    fn tcp_nodelay(slf: PyRefMut<Self>, enabled: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tcp_nodelay(enabled)))
    }

    fn local_address(slf: PyRefMut<Self>, addr: Option<String>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| {
            let addr = addr
                .map(|v| IpAddr::from_str(v.as_str()))
                .transpose()
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(builder.local_address(addr))
        })
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn interface(slf: PyRefMut<Self>, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.interface(value.as_str())))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn interface(slf: PyRefMut<Self>, value: String) -> PyResult<PyRefMut<Self>> {
        Err(PyValueError::new_err("interface is not supported on this platform"))
    }

    fn tcp_keepalive(slf: PyRefMut<Self>, value: Option<Duration>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tcp_keepalive(value)))
    }

    fn tcp_keepalive_interval(slf: PyRefMut<Self>, value: Option<Duration>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tcp_keepalive_interval(value)))
    }

    fn tcp_keepalive_retries(slf: PyRefMut<Self>, value: Option<u32>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tcp_keepalive_retries(value)))
    }

    #[cfg(target_os = "linux")]
    fn tcp_user_timeout(slf: PyRefMut<Self>, timeout: Option<Duration>) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tcp_user_timeout(timeout)))
    }

    #[cfg(not(target_os = "linux"))]
    fn tcp_user_timeout(_slf: PyRefMut<Self>, _timeout: Option<Duration>) -> PyResult<PyRefMut<Self>> {
        Err(PyValueError::new_err("tcp_user_timeout is not supported on this platform"))
    }

    fn add_root_certificate_der(slf: PyRefMut<Self>, cert: PyBytes) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| {
            let cert =
                reqwest::Certificate::from_der(cert.as_slice()).map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(builder.add_root_certificate(cert))
        })
    }

    fn add_root_certificate_pem(slf: PyRefMut<Self>, cert: PyBytes) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| {
            let cert =
                reqwest::Certificate::from_pem(cert.as_slice()).map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(builder.add_root_certificate(cert))
        })
    }

    fn add_crl_pem(slf: PyRefMut<Self>, cert: PyBytes) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| {
            let cert = reqwest::tls::CertificateRevocationList::from_pem(cert.as_slice())
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(builder.add_crl(cert))
        })
    }

    fn tls_built_in_root_certs(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tls_built_in_root_certs(enable)))
    }

    fn tls_built_in_webpki_certs(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tls_built_in_webpki_certs(enable)))
    }

    fn identity_pem(slf: PyRefMut<Self>, buf: PyBytes) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| {
            let identity =
                reqwest::Identity::from_pem(buf.as_slice()).map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(builder.identity(identity))
        })
    }

    fn danger_accept_invalid_hostnames(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.danger_accept_invalid_hostnames(enable)))
    }

    fn danger_accept_invalid_certs(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.danger_accept_invalid_certs(enable)))
    }

    fn tls_sni(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.tls_sni(enable)))
    }

    fn min_tls_version(slf: PyRefMut<Self>, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.min_tls_version(Self::parse_tls_version(value.as_str())?)))
    }

    fn max_tls_version(slf: PyRefMut<Self>, value: String) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.max_tls_version(Self::parse_tls_version(value.as_str())?)))
    }

    fn https_only(slf: PyRefMut<Self>, enable: bool) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| Ok(builder.https_only(enable)))
    }

    fn resolve(slf: PyRefMut<Self>, domain: String, ip: String, port: u16) -> PyResult<PyRefMut<Self>> {
        Self::apply(slf, |builder| {
            let ip = IpAddr::from_str(ip.as_str()).map_err(|e| PyValueError::new_err(e.to_string()))?;
            Ok(builder.resolve(domain.as_str(), SocketAddr::new(ip, port)))
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        if let Some(middlewares) = &self.middlewares {
            for mw in middlewares.iter() {
                visit.call(mw)?;
            }
        }
        if let Some(json_handler) = &self.json_handler {
            json_handler.__traverse__(&visit)?;
        }
        visit.call(&self.runtime)
    }

    fn __clear__(&mut self) {
        self.middlewares = None;
        self.json_handler = None;
        self.runtime = None;
    }
}
impl BaseClientBuilder {
    fn new() -> Self {
        Self {
            inner: Some(reqwest::ClientBuilder::new()),
            ..Default::default()
        }
    }

    fn build_client_base(&mut self, py: Python) -> PyResult<BaseClient> {
        let runtime = match self.runtime.take() {
            Some(runtime) => runtime.try_borrow(py)?.handle().clone(),
            None => RuntimeHandle::global_handle()?.clone(),
        };

        py.detach(|| {
            let inner_client = self
                .inner
                .take()
                .ok_or_else(|| PyRuntimeError::new_err("Client was already built"))?
                .use_rustls_tls()
                .build()
                .map_err(|e| PyValueError::new_err(e.to_string()))?;

            let client = BaseClient::new(
                inner_client,
                runtime,
                self.middlewares.take(),
                self.json_handler.take(),
                self.total_timeout,
                self.max_connections
                    .map(|max| ConnectionLimiter::new(max, self.pool_timeout)),
                self.error_for_status,
                self.default_headers.take(),
                self.base_url.take(),
            );
            Ok(client)
        })
    }

    fn apply<F>(mut slf: PyRefMut<Self>, fun: F) -> PyResult<PyRefMut<Self>>
    where
        F: FnOnce(reqwest::ClientBuilder) -> PyResult<reqwest::ClientBuilder>,
        F: Send,
    {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Client was already built"))?;
        slf.inner = Some(slf.py().detach(|| fun(builder))?);
        Ok(slf)
    }

    fn check_inner(&self) -> PyResult<()> {
        self.inner
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("Client was already built"))
            .map(|_| ())
    }

    fn parse_tls_version(version: &str) -> PyResult<reqwest::tls::Version> {
        match version {
            "TLSv1.0" => Ok(reqwest::tls::Version::TLS_1_0),
            "TLSv1.1" => Ok(reqwest::tls::Version::TLS_1_1),
            "TLSv1.2" => Ok(reqwest::tls::Version::TLS_1_2),
            "TLSv1.3" => Ok(reqwest::tls::Version::TLS_1_3),
            _ => Err(PyValueError::new_err(
                "Invalid TLS version. Use 'TLSv1.0', 'TLSv1.1', 'TLSv1.2', or 'TLSv1.3'",
            )),
        }
    }
}

#[pymethods]
impl ClientBuilder {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(BaseClientBuilder::new()).add_subclass(Self)
    }

    fn build(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<Client>> {
        Client::new_py(py, slf.as_super().build_client_base(py)?)
    }
}

#[pymethods]
impl SyncClientBuilder {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        PyClassInitializer::from(BaseClientBuilder::new()).add_subclass(Self)
    }

    fn build(mut slf: PyRefMut<Self>, py: Python) -> PyResult<Py<SyncClient>> {
        SyncClient::new_py(py, slf.as_super().build_client_base(py)?)
    }
}
