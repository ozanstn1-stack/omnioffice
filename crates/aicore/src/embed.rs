//! Embeddings: one text, or many texts in as few requests as the provider allows.
//!
//! OpenAI-compatible endpoints (OpenAI, Gemini's compatibility layer, Mistral,
//! DeepSeek-style gateways) accept an `input` array and return one vector per
//! entry, each tagged with its `index`. Ollama's `/api/embeddings` takes a
//! single `prompt`, so a batch is sent one text at a time there, which also
//! keeps older Ollama versions working.

use super::{map_http_error, retry_after_seconds, AiError, AiResult, DeepSeekClient, ProviderKind};
use serde::{Deserialize, Serialize};

/// Most texts sent in one embeddings request; larger batches are split.
pub const EMBED_BATCH: usize = 64;

#[derive(Deserialize)]
struct OllamaEmbedResp {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct OpenAiData {
    embedding: Vec<f32>,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Deserialize)]
struct OpenAiEmbedResp {
    data: Vec<OpenAiData>,
}

/// Orders the vectors by their `index` (providers may return them in any
/// order) and checks there is exactly one per input.
fn ordered_vectors(mut data: Vec<OpenAiData>, expected: usize) -> Option<Vec<Vec<f32>>> {
    if data.len() != expected {
        return None;
    }
    if data.iter().all(|item| item.index.is_some()) {
        data.sort_by_key(|item| item.index);
        if data.iter().enumerate().any(|(position, item)| item.index != Some(position)) {
            return None;
        }
    }
    Some(data.into_iter().map(|item| item.embedding).collect())
}

impl DeepSeekClient {
    /// Endpoint for embeddings computation.
    fn embeddings_endpoint(&self) -> String {
        let base = self.config.base_url.trim().trim_end_matches('/');
        match self.provider() {
            ProviderKind::Ollama => format!("{base}/api/embeddings"),
            ProviderKind::Gemini => format!("{base}/openai/embeddings"),
            _ => format!("{base}/embeddings"),
        }
    }

    fn embedding_model(&self) -> &str {
        self.config.embedding_model.as_deref().unwrap_or("nomic-embed-text")
    }

    /// Sends one embeddings request and returns the response body.
    async fn embeddings_request<T: Serialize + ?Sized>(&self, payload: &T) -> AiResult<String> {
        let request = self.authorize(self.http.post(self.embeddings_endpoint())).json(payload);
        let response = request.send().await.map_err(|error| self.unreachable(error))?;
        let status = response.status();
        let retry_after = retry_after_seconds(&response);
        let body = self.response_text(response).await?;
        if !status.is_success() {
            return Err(map_http_error(status.as_u16(), &body, &self.provider_name(), retry_after));
        }
        Ok(body)
    }

    /// Computes embeddings vector for the provided text.
    pub async fn embed(&self, text: &str) -> AiResult<Vec<f32>> {
        let model = self.embedding_model();
        let body = match self.provider() {
            ProviderKind::Ollama => {
                #[derive(Serialize)]
                struct OllamaEmbedReq<'a> {
                    model: &'a str,
                    prompt: &'a str,
                }
                self.embeddings_request(&OllamaEmbedReq { model, prompt: text }).await?
            }
            _ => {
                #[derive(Serialize)]
                struct OpenAiEmbedReq<'a> {
                    model: &'a str,
                    input: &'a str,
                }
                self.embeddings_request(&OpenAiEmbedReq { model, input: text }).await?
            }
        };
        if let Ok(resp) = serde_json::from_str::<OllamaEmbedResp>(&body) {
            return Ok(resp.embedding);
        }
        if let Ok(mut resp) = serde_json::from_str::<OpenAiEmbedResp>(&body) {
            if let Some(first) = resp.data.pop() {
                return Ok(first.embedding);
            }
        }
        Err(AiError::InvalidResponse(self.provider_name()))
    }

    /// Embeds every text, in input order: OpenAI-compatible providers get
    /// one request per [`EMBED_BATCH`] texts, Ollama one per text. The first
    /// error stops the run, so the caller keeps nothing half-matched.
    pub async fn embed_batch(&self, texts: &[&str]) -> AiResult<Vec<Vec<f32>>> {
        if self.provider() == ProviderKind::Ollama {
            let mut vectors = Vec::with_capacity(texts.len());
            for text in texts {
                vectors.push(self.embed(text).await?);
            }
            return Ok(vectors);
        }
        #[derive(Serialize)]
        struct OpenAiBatchReq<'a> {
            model: &'a str,
            input: &'a [&'a str],
        }
        let model = self.embedding_model();
        let mut vectors = Vec::with_capacity(texts.len());
        for batch in texts.chunks(EMBED_BATCH) {
            let body = self.embeddings_request(&OpenAiBatchReq { model, input: batch }).await?;
            let parsed = serde_json::from_str::<OpenAiEmbedResp>(&body)
                .ok()
                .and_then(|resp| ordered_vectors(resp.data, batch.len()))
                .ok_or_else(|| AiError::InvalidResponse(self.provider_name()))?;
            vectors.extend(parsed);
        }
        Ok(vectors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: Option<usize>, value: f32) -> OpenAiData {
        OpenAiData { embedding: vec![value], index }
    }

    #[test]
    fn vectors_are_put_back_in_input_order() {
        let data = vec![item(Some(2), 2.0), item(Some(0), 0.0), item(Some(1), 1.0)];
        assert_eq!(ordered_vectors(data, 3), Some(vec![vec![0.0], vec![1.0], vec![2.0]]));
        // Without indices the response order is kept.
        assert_eq!(ordered_vectors(vec![item(None, 5.0), item(None, 6.0)], 2), Some(vec![vec![5.0], vec![6.0]]));
    }

    #[test]
    fn a_missing_or_duplicated_vector_is_rejected() {
        assert_eq!(ordered_vectors(vec![item(Some(0), 0.0)], 2), None);
        assert_eq!(ordered_vectors(vec![item(Some(0), 0.0), item(Some(0), 1.0)], 2), None);
    }
}
