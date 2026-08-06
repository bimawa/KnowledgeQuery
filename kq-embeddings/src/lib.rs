use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::{Linear, Module, VarBuilder};
use candle_transformers::models::bert::{Config as BertConfig, HiddenAct};
use hf_hub::api::sync::ApiBuilder;
use tokenizers::Tokenizer;

const MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";

type CResult<T> = std::result::Result<T, candle_core::Error>;

fn ce(err: candle_core::Error) -> anyhow::Error {
    anyhow::anyhow!(err)
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ModelConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub intermediate_size: usize,
    pub hidden_act: String,
    pub max_position_embeddings: usize,
    pub layer_norm_eps: f64,
    pub pad_token_id: Option<usize>,
}

impl ModelConfig {
    fn to_bert_config(&self) -> BertConfig {
        let hidden_act = match self.hidden_act.as_str() {
            "gelu" => HiddenAct::GeluApproximate,
            "gelu_pytorch_tanh" => HiddenAct::Gelu,
            _ => HiddenAct::GeluApproximate,
        };
        BertConfig {
            vocab_size: self.vocab_size,
            hidden_size: self.hidden_size,
            num_hidden_layers: self.num_hidden_layers,
            num_attention_heads: self.num_attention_heads,
            intermediate_size: self.intermediate_size,
            hidden_act,
            hidden_dropout_prob: 0.1,
            max_position_embeddings: self.max_position_embeddings,
            type_vocab_size: 2,
            initializer_range: 0.02,
            layer_norm_eps: self.layer_norm_eps,
            pad_token_id: self.pad_token_id.unwrap_or(0),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct SentenceTransformerConfig {
    /// Flat BERT config fields (hidden_size, num_hidden_layers, etc.)
    #[serde(flatten)]
    pub transformer: ModelConfig,
    #[serde(default)]
    pub projection_dim: Option<usize>,
}

struct SentenceTransformer {
    embeddings: Embeddings,
    attention: Vec<EncoderLayer>,
}

struct Embeddings {
    word_embeddings: Tensor,
    position_embeddings: Tensor,
    token_type_embeddings: Tensor,
    layer_norm: LayerNorm,
}

impl Embeddings {
    fn forward(&self, input_ids: &Tensor, token_type_ids: &Tensor) -> CResult<Tensor> {
        let seq_len = input_ids.dim(1)?;
        let input_ids_flat = input_ids.reshape(((),))?;
        let word_emb =
            self.word_embeddings.index_select(&input_ids_flat, 0)?.reshape((input_ids.dim(0)?, seq_len, ()))?;

        let position_ids = Tensor::arange(0u32, seq_len as u32, input_ids.device())?.to_dtype(DType::I64)?;
        let pos_emb = self.position_embeddings.index_select(&position_ids, 0)?.unsqueeze(0)?;

        let token_type_flat = token_type_ids.reshape(((),))?;
        let token_type_emb = self.token_type_embeddings.index_select(&token_type_flat, 0)?.reshape((
            token_type_ids.dim(0)?,
            seq_len,
            (),
        ))?;

        let embeddings = ((&word_emb + &pos_emb)? + &token_type_emb)?;
        self.layer_norm.forward(&embeddings)
    }
}

struct LayerNorm {
    weight: Tensor,
    bias: Tensor,
    eps: f64,
}

impl LayerNorm {
    fn forward(&self, x: &Tensor) -> CResult<Tensor> {
        let dims = x.dims();
        let last_dim = dims[dims.len() - 1];
        let x_f32 = x.to_dtype(DType::F32)?;

        let mean = x_f32.mean_keepdim(candle_core::D::Minus1)?;
        let centered = x_f32.broadcast_sub(&mean)?;
        let variance = centered.sqr()?.mean_keepdim(candle_core::D::Minus1)?;
        let x_norm = centered.broadcast_div(&((&variance + self.eps)?.sqrt()?))?;

        let w = self.weight.to_dtype(DType::F32)?.reshape((1, 1, last_dim))?;
        let b = self.bias.to_dtype(DType::F32)?.reshape((1, 1, last_dim))?;
        let result = x_norm.broadcast_mul(&w)?.broadcast_add(&b)?;
        result.to_dtype(x.dtype())
    }
}

struct EncoderLayer {
    self_attn: MultiHeadAttention,
    self_attn_layer_norm: LayerNorm,
    ffn: FeedForward,
    ffn_layer_norm: LayerNorm,
}

struct MultiHeadAttention {
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
    num_heads: usize,
    head_dim: usize,
}

impl MultiHeadAttention {
    fn forward(&self, x: &Tensor, attention_mask: &Tensor) -> CResult<Tensor> {
        let (batch, seq_len, _) = x.dims3()?;
        let q = self.q.forward(x)?;
        let k = self.k.forward(x)?;
        let v = self.v.forward(x)?;

        let q = q.reshape((batch, seq_len, self.num_heads, self.head_dim))?.transpose(1, 2)?;
        let k = k.reshape((batch, seq_len, self.num_heads, self.head_dim))?.transpose(1, 2)?;
        let v = v.reshape((batch, seq_len, self.num_heads, self.head_dim))?.transpose(1, 2)?;

        let scale = (self.head_dim as f64).sqrt();
        let attn_weights = (q.matmul(&k.transpose(candle_core::D::Minus2, candle_core::D::Minus1)?)? / scale)?;
        let attn_weights = attn_weights.broadcast_add(attention_mask)?;
        let attn_weights = candle_nn::ops::softmax(&attn_weights, candle_core::D::Minus1)?;

        let attn_output = attn_weights.matmul(&v)?;
        let attn_output = attn_output.transpose(1, 2)?.reshape((batch, seq_len, ()))?;
        self.o.forward(&attn_output)
    }
}

struct FeedForward {
    dense: Linear,
    dense_out: Linear,
    act: Activation,
}

impl FeedForward {
    fn forward(&self, x: &Tensor) -> CResult<Tensor> {
        let x = self.dense.forward(x)?;
        let x = self.act.forward(&x)?;
        self.dense_out.forward(&x)
    }
}

#[derive(Clone, Copy)]
enum Activation {
    Gelu,
}

impl Activation {
    fn forward(&self, x: &Tensor) -> CResult<Tensor> {
        match self {
            Activation::Gelu => x.gelu(),
        }
    }
}

pub struct EmbeddingModel {
    cache_dir: std::path::PathBuf,
    model: Option<SentenceTransformer>,
    tokenizer: Option<Tokenizer>,
    device: Device,
}

impl EmbeddingModel {
    pub fn new(cache_dir: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(cache_dir).context("Failed to create cache directory")?;
        Ok(Self { cache_dir: cache_dir.to_path_buf(), model: None, tokenizer: None, device: Device::Cpu })
    }

    pub fn load(&mut self) -> Result<()> {
        let api = ApiBuilder::new()
            .with_cache_dir(self.cache_dir.clone())
            .build()
            .context("Failed to create HuggingFace API client")?;

        let tokenizer_path =
            api.model(MODEL_ID.to_string()).download("tokenizer.json").context("Failed to download tokenizer.json")?;
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("Failed to load tokenizer")?;
        self.tokenizer = Some(tokenizer);

        let config_path =
            api.model(MODEL_ID.to_string()).download("config.json").context("Failed to download config.json")?;
        let config_str = std::fs::read_to_string(&config_path)?;
        let st_config: SentenceTransformerConfig =
            serde_json::from_str(&config_str).context("Failed to parse config.json")?;

        let model_path = api
            .model(MODEL_ID.to_string())
            .download("model.safetensors")
            .context("Failed to download model.safetensors")?;

        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[&model_path], DType::F32, &self.device).map_err(ce)? };

        let model = self.load_sentence_transformer(vb, &st_config).map_err(ce)?;
        self.model = Some(model);

        Ok(())
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.model.as_ref().context("Model not loaded; call load() first")?;
        let tokenizer = self.tokenizer.as_ref().context("Tokenizer not loaded; call load() first")?;

        let encoding =
            tokenizer.encode(text, true).map_err(|e| anyhow::anyhow!("{e}")).context("Failed to encode text")?;

        let ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        let input_ids = Tensor::new(ids, &self.device).map_err(ce)?.unsqueeze(0).map_err(ce)?;
        let attention_mask = Tensor::new(attention_mask, &self.device).map_err(ce)?.unsqueeze(0).map_err(ce)?;
        let token_type_ids = Tensor::zeros(input_ids.shape(), DType::I64, &self.device).map_err(ce)?;

        let sequence_output = model.forward(&input_ids, &token_type_ids, &attention_mask).map_err(ce)?;
        let embeddings = self.mean_pool(&sequence_output, &attention_mask).map_err(ce)?;

        let norm = embeddings.sqr().map_err(ce)?.sum_keepdim(candle_core::D::Minus1).map_err(ce)?.sqrt().map_err(ce)?;
        let embeddings = embeddings.broadcast_div(&norm).map_err(ce)?;

        let result: Vec<f32> = embeddings.squeeze(0)?.to_vec1().map_err(ce)?;
        Ok(result)
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }

    fn mean_pool(&self, hidden_states: &Tensor, attention_mask: &Tensor) -> CResult<Tensor> {
        let attention_mask_expanded = attention_mask.unsqueeze(candle_core::D::Minus1)?.to_dtype(DType::F32)?;

        let masked = hidden_states.broadcast_mul(&attention_mask_expanded)?;
        let summed = masked.sum(1)?;
        let counts = attention_mask_expanded.sum(1)?.clamp(1e-9, f32::INFINITY)?;
        let d0 = summed.dim(0)?;
        let d1 = summed.dim(1)?;
        summed / counts.expand(&[d0, d1])?
    }

    fn load_sentence_transformer(
        &self,
        vb: VarBuilder,
        config: &SentenceTransformerConfig,
    ) -> CResult<SentenceTransformer> {
        let bert_config = config.transformer.to_bert_config();

        let word_embeddings = vb
            .get((bert_config.vocab_size, bert_config.hidden_size), "embeddings.word_embeddings.weight")?
            .to_dtype(DType::F32)?;

        let max_pos = bert_config.max_position_embeddings;
        let position_embeddings = vb
            .get((max_pos, bert_config.hidden_size), "embeddings.position_embeddings.weight")?
            .to_dtype(DType::F32)?;

        let token_type_embeddings = vb
            .get((bert_config.type_vocab_size, bert_config.hidden_size), "embeddings.token_type_embeddings.weight")?
            .to_dtype(DType::F32)?;

        let ln_weight = vb.get(bert_config.hidden_size, "embeddings.LayerNorm.weight")?.to_dtype(DType::F32)?;
        let ln_bias = vb.get(bert_config.hidden_size, "embeddings.LayerNorm.bias")?.to_dtype(DType::F32)?;

        let embeddings = Embeddings {
            word_embeddings,
            position_embeddings,
            token_type_embeddings,
            layer_norm: LayerNorm { weight: ln_weight, bias: ln_bias, eps: bert_config.layer_norm_eps },
        };

        let mut attention_layers = Vec::with_capacity(bert_config.num_hidden_layers);
        for i in 0..bert_config.num_hidden_layers {
            let prefix = format!("encoder.layer.{i}");

            let q = Linear::new(
                vb.get(
                    (bert_config.hidden_size, bert_config.hidden_size),
                    &format!("{prefix}.attention.self.query.weight"),
                )?,
                Some(vb.get(bert_config.hidden_size, &format!("{prefix}.attention.self.query.bias"))?),
            );
            let k = Linear::new(
                vb.get(
                    (bert_config.hidden_size, bert_config.hidden_size),
                    &format!("{prefix}.attention.self.key.weight"),
                )?,
                Some(vb.get(bert_config.hidden_size, &format!("{prefix}.attention.self.key.bias"))?),
            );
            let v = Linear::new(
                vb.get(
                    (bert_config.hidden_size, bert_config.hidden_size),
                    &format!("{prefix}.attention.self.value.weight"),
                )?,
                Some(vb.get(bert_config.hidden_size, &format!("{prefix}.attention.self.value.bias"))?),
            );
            let o = Linear::new(
                vb.get(
                    (bert_config.hidden_size, bert_config.hidden_size),
                    &format!("{prefix}.attention.output.dense.weight"),
                )?,
                Some(vb.get(bert_config.hidden_size, &format!("{prefix}.attention.output.dense.bias"))?),
            );

            let num_heads = bert_config.num_attention_heads;
            let head_dim = bert_config.hidden_size / num_heads;

            let sa_ln_w = vb.get(bert_config.hidden_size, &format!("{prefix}.attention.output.LayerNorm.weight"))?;
            let sa_ln_b = vb.get(bert_config.hidden_size, &format!("{prefix}.attention.output.LayerNorm.bias"))?;

            let ffn_w1 = format!("{prefix}.intermediate.dense.weight");
            let ffn_b1 = format!("{prefix}.intermediate.dense.bias");
            let ffn_w2 = format!("{prefix}.output.dense.weight");
            let ffn_b2 = format!("{prefix}.output.dense.bias");
            let ffn_ln_w = format!("{prefix}.output.LayerNorm.weight");
            let ffn_ln_b = format!("{prefix}.output.LayerNorm.bias");

            let ffn_dense = Linear::new(
                vb.get((bert_config.intermediate_size, bert_config.hidden_size), &ffn_w1)?,
                Some(vb.get(bert_config.intermediate_size, &ffn_b1)?),
            );
            let ffn_out = Linear::new(
                vb.get((bert_config.hidden_size, bert_config.intermediate_size), &ffn_w2)?,
                Some(vb.get(bert_config.hidden_size, &ffn_b2)?),
            );

            attention_layers.push(EncoderLayer {
                self_attn: MultiHeadAttention { q, k, v, o, num_heads, head_dim },
                self_attn_layer_norm: LayerNorm {
                    weight: sa_ln_w.to_dtype(DType::F32)?,
                    bias: sa_ln_b.to_dtype(DType::F32)?,
                    eps: bert_config.layer_norm_eps,
                },
                ffn: FeedForward { dense: ffn_dense, dense_out: ffn_out, act: Activation::Gelu },
                ffn_layer_norm: LayerNorm {
                    weight: vb.get(bert_config.hidden_size, &ffn_ln_w)?.to_dtype(DType::F32)?,
                    bias: vb.get(bert_config.hidden_size, &ffn_ln_b)?.to_dtype(DType::F32)?,
                    eps: bert_config.layer_norm_eps,
                },
            });
        }

        Ok(SentenceTransformer { embeddings, attention: attention_layers })
    }
}

impl SentenceTransformer {
    fn forward(&self, input_ids: &Tensor, token_type_ids: &Tensor, attention_mask: &Tensor) -> CResult<Tensor> {
        let hidden_states = self.embeddings.forward(input_ids, token_type_ids)?;

        let attention_mask_2d = attention_mask.to_dtype(DType::F32)?;
        let extended_mask = attention_mask_2d.unsqueeze(1)?.unsqueeze(1)?;
        let extended_mask = ((extended_mask.ones_like()? - &extended_mask)? * f64::MIN)?;

        let mut x = hidden_states;
        for layer in &self.attention {
            let residual = x.clone();
            let attn_out = layer.self_attn.forward(&x, &extended_mask)?;
            x = layer.self_attn_layer_norm.forward(&(residual + attn_out)?)?;

            let residual = x.clone();
            let ffn_out = layer.ffn.forward(&x)?;
            x = layer.ffn_layer_norm.forward(&(residual + ffn_out)?)?;
        }

        Ok(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_cache_dir() -> PathBuf {
        let mut dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()));
        dir.push("test_cache");
        dir
    }

    #[test]
    fn embedding_model_new() {
        let cache_dir = test_cache_dir();
        let model = EmbeddingModel::new(&cache_dir);
        assert!(model.is_ok());
    }

    #[test]
    fn embedding_model_embed_without_load_fails() {
        let cache_dir = test_cache_dir();
        let model = EmbeddingModel::new(&cache_dir).unwrap();
        let result = model.embed("hello world");
        assert!(result.is_err());
    }

    #[test]
    fn embedding_model_embed_batch_without_load_fails() {
        let cache_dir = test_cache_dir();
        let model = EmbeddingModel::new(&cache_dir).unwrap();
        let result = model.embed_batch(&["hello", "world"]);
        assert!(result.is_err());
    }
}
