// This file is part of HTTP Signatures

// HTTP Signatures is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// HTTP Signatures is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with HTTP Signatures  If not, see <http://www.gnu.org/licenses/>.

//! Available with the `use_hyper` feature. This modulde defines the `AsHttpSignature` and
//! `WithHttpSignature` traits for `hyper::Request`.
//!
//! This is useful for interacting with HTTP Signatures from Hyper-based applications, since it can
//! automatically generate signatures and add them to requests.
//!
//! # Example generating a signature
//! This example shows getting an `HttpSignature` type from a `hyper::Request`. Typically you
//! wouldn't want to do this directly, you'd use `with_authorization_header` or
//! `with_signature_header` directly, but in the event you want the intermediate state, this is
//! available.
//!
//! ```rust
//! # #![feature(try_from)]
//! # extern crate hyper;
//! # extern crate http_signatures;
//! #
//! # use std::convert::TryInto;
//! # use std::error::Error;
//! # use std::fs::File;
//! #
//! # use http_signatures::prelude::*;
//! # use http_signatures::{ShaSize, SignatureAlgorithm};
//! # use hyper::{Method, Request};
//! #
//! # fn run() -> Result<(), Box<Error>> {
//! let key = File::open("tests/assets/private.der")?;
//! let uri = "https://example.com".parse()?;
//! let alg = SignatureAlgorithm::RSA(ShaSize::TwoFiftySix);
//!
//! let req: Request = Request::new(Method::Post, uri);
//!
//! let http_sig = req.as_http_signature("rsa-key-1".into(), key, alg)?;
//! #     Ok(())
//! # }
//! # fn main() {
//! #     run().unwrap();
//! # }
//! ```
//!
//! # Example adding a signature to a Request type
//!
//! This example adds the HTTP Signature to the request directly as an Authorization header.
//! `with_signature_header` can be used to add the signature as a Signature header instead.
//!
//! ```rust
//! # extern crate hyper;
//! # extern crate http_signatures;
//! #
//! # use std::error::Error;
//! # use std::fs::File;
//! #
//! # use http_signatures::prelude::*;
//! # use http_signatures::{ShaSize, SignatureAlgorithm};
//! # use hyper::{Method, Request};
//! #
//! # fn run() -> Result<(), Box<Error>> {
//! let key = File::open("tests/assets/private.der")?;
//! let uri = "https://example.com".parse()?;
//! let alg = SignatureAlgorithm::RSA(ShaSize::TwoFiftySix);
//!
//! let mut req: Request = Request::new(Method::Post, uri);
//!
//! req.with_authorization_header("rsa-key-1".into(), key, alg)?;
//! #     Ok(())
//! # }
//! # fn main() {
//! #     run().unwrap();
//! # }
//! ```
//!
//! See
//! [this example](https://github.com/asonix/http-signatures/blob/master/examples/hyper_client.rs)
//! for more information.

use std::io::Read;
use std::collections::BTreeMap;

use create::HttpSignature;
use error::Error;
use prelude::*;
use super::{SignatureAlgorithm, REQUEST_TARGET};

use hyper::Request as HyperRequest;

/// An implementation of `AsHttpSignature` for `hyper::Request`.
///
/// This trait is not often used directly, but is required by the `WithHttpSignature` trait defined
/// below.
impl<T> AsHttpSignature<T> for HyperRequest
where
    T: Read,
{
    fn as_http_signature(
        &self,
        key_id: String,
        key: T,
        algorithm: SignatureAlgorithm,
    ) -> Result<HttpSignature<T>, Error> {
        let mut headers = BTreeMap::new();
        headers.insert(
            REQUEST_TARGET.into(),
            vec![
                if let Some(query) = self.uri().query() {
                    format!(
                        "{} {}?{}",
                        self.method().as_ref().to_lowercase(),
                        self.uri().path(),
                        query
                    )
                } else {
                    format!(
                        "{} {}",
                        self.method().as_ref().to_lowercase(),
                        self.uri().path()
                    )
                },
            ],
        );

        let headers = self.headers().iter().fold(headers, |mut acc, header_view| {
            acc.entry(header_view.name().into())
                .or_insert_with(Vec::new)
                .push(header_view.value_string());

            acc
        });

        HttpSignature::new(key_id, key, algorithm, headers).map_err(Error::from)
    }
}

/// An implementation of `WithHttpSignature` for `hyper::Request`
///
/// This automatically adds an Authorization header to a given `hyper::Request` struct containing
/// an HTTP Signature.
///
/// See
/// [this example](https://github.com/asonix/http-signatures/blob/master/examples/hyper_client.rs)
/// for usage information.
impl<T> WithHttpSignature<T> for HyperRequest
where
    T: Read,
{
    fn with_authorization_header(
        &mut self,
        key_id: String,
        key: T,
        algorithm: SignatureAlgorithm,
    ) -> Result<&mut Self, Error> {
        use hyper::header::Authorization;

        let auth_header = self.authorization_header(key_id, key, algorithm)?;
        self.headers_mut().set(Authorization(auth_header));

        Ok(self)
    }

    fn with_signature_header(
        &mut self,
        key_id: String,
        key: T,
        algorithm: SignatureAlgorithm,
    ) -> Result<&mut Self, Error> {
        let sig_header = self.signature_header(key_id, key, algorithm)?;
        self.headers_mut().set_raw("Signature", sig_header);

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;
    use std::fs::File;
    use std::str::FromStr;

    use hyper::{Method, Request};
    use hyper::header::{ContentLength, ContentType, Date, Host, HttpDate};

    use create::SigningString;
    use ShaSize;
    use SignatureAlgorithm;
    use prelude::*;

    /* Request used for all tests:
     *
     * POST /foo HTTP/1.1
     * Host: example.org
     * Date: Tue, 07 Jun 2014 20:51:35 GMT
     * Content-Type: application/json
     * Digest: SHA-256=X48E9qOokqqrvdts8nOJRJN3OWDUoyWxBf7kbu9DBPE=
     * Content-Length: 18
     *
     * {"hello": "world"}
     */

    const KEY_ID: &'static str = "rsa-key-1";
    const ALGORITHM: SignatureAlgorithm = SignatureAlgorithm::RSA(ShaSize::TwoFiftySix);
    const PRIVATE_KEY_PATH: &'static str = "tests/assets/private.der";

    #[test]
    fn min_test() {
        let uri = "http://example.org/foo".parse().unwrap();
        let req = Request::new(Method::Post, uri);

        test_request(req, "(request-target): post /foo");
    }

    #[test]
    fn full_test() {
        let uri = "http://example.org/foo".parse().unwrap();
        let mut req = Request::new(Method::Post, uri);

        req.headers_mut().set(Host::new("example.org", None));
        req.headers_mut().set(ContentType::json());
        req.headers_mut().set_raw(
            "Digest",
            "SHA-256=X48E9qOokqqrvdts8nOJRJN3OWDUoyWxBf7kbu9DBPE=",
        );
        req.headers_mut().set(Date(
            HttpDate::from_str("Tue, 07 Jun 2014 20:51:35 GMT").unwrap(),
        ));
        req.headers_mut().set(ContentLength(18));
        req.set_body(r#"{"hello": "world"}"#);

        test_request(
            req,
            "(request-target): post /foo
content-length: 18
content-type: application/json
date: Tue, 07 Jun 2014 20:51:35 GMT
digest: SHA-256=X48E9qOokqqrvdts8nOJRJN3OWDUoyWxBf7kbu9DBPE=
host: example.org",
        )
    }

    fn test_request(req: Request, s: &str) {
        let key = File::open(PRIVATE_KEY_PATH).unwrap();

        let http_sig = req.as_http_signature(KEY_ID.into(), key, ALGORITHM)
            .unwrap();

        let signing_string: SigningString<File> = http_sig.try_into().unwrap();

        assert_eq!(signing_string.signing_string, s);
    }
}
